//! Project-configuration resolution for the cheap per-edit syntax checks.
//!
//! The per-edit syntax gate (`VerificationGate::run_cheap_syntax_check`)
//! invokes language tools DIRECTLY on the edited files. A direct invocation
//! ignores the project's own configuration, so the tool falls back to its
//! built-in defaults — an old language level — and rejects valid modern code
//! as a "syntax error". 0.8.1 fixed this for Rust (see
//! [`super::rust_edition`]); this module is the same fix for every other
//! language the gate checks:
//!
//! | Language   | Tool default that caused false failures      | Resolved from |
//! |------------|----------------------------------------------|---------------|
//! | TypeScript | `tsc <files>` ignores `tsconfig.json` (ES5 target/lib, no JSX, no decorators) | nearest `tsconfig.json` (checked through a temporary config that `extends` it and lists only the edited files) |
//! | JavaScript | `node --check` parses ESM `.js` as CommonJS; cannot parse JSX | `.mjs`/`.cjs`, nearest `package.json` `"type"`, ESM syntax detection; JSX via `tsc --allowJs` |
//! | C / C++    | compiler default `-std` (pre-C++20)           | `compile_commands.json` entry → `CMakeLists.txt` standard → modern default |
//! | Python     | host `python3` may be older than the project  | `.python-version`, `pyproject.toml` `requires-python`, `setup.cfg` `python_requires` |
//! | Java       | host `javac` default release                  | `pom.xml` / `build.gradle(.kts)` release / source level |
//!
//! Every resolver is pure (filesystem reads only, no process spawns) so it is
//! unit-testable with temp dirs, and every resolution records WHERE its value
//! came from so the gate can say what it actually checked (AGENTS.md Rule 3).
//! Resolvers only walk directories between the file and the project root
//! (inclusive) when the file lies inside the project, so a config file above
//! the workspace (e.g. in `$HOME`) never leaks into a check.

use std::path::{Path, PathBuf};

/// Ancestor directories of `file` (nearest first), bounded to `root`
/// (inclusive) when `file` lies under `root`; otherwise every ancestor.
pub fn ancestor_dirs(file: &Path, root: &Path) -> Vec<PathBuf> {
    let start = if file.is_dir() {
        Some(file)
    } else {
        file.parent()
    };
    let Some(start) = start else {
        return Vec::new();
    };
    let bounded = start.starts_with(root);
    let mut out = Vec::new();
    for dir in start.ancestors() {
        out.push(dir.to_path_buf());
        if bounded && dir == root {
            break;
        }
    }
    out
}

fn first_file_in(dirs: &[PathBuf], names: &[&str]) -> Option<PathBuf> {
    dirs.iter()
        .flat_map(|d| names.iter().map(move |n| d.join(n)))
        .find(|p| p.is_file())
}

/// Name of an executable inside `node_modules/.bin` on this platform.
fn bin_name(name: &str) -> String {
    if cfg!(windows) {
        format!("{name}.cmd")
    } else {
        name.to_string()
    }
}

// ─────────────────────────────── TypeScript ───────────────────────────────

/// The nearest `tsconfig.json` above `file`, within the project.
pub fn find_tsconfig(file: &Path, root: &Path) -> Option<PathBuf> {
    first_file_in(&ancestor_dirs(file, root), &["tsconfig.json"])
}

/// The nearest project-local `node_modules/.bin/<name>` above `start`.
///
/// Deliberately walks past the project root (monorepos hoist `node_modules`
/// to the repository root). This replaces `npx`, which silently DOWNLOADS a
/// package when it is not installed — a network install inside a syntax gate.
pub fn find_local_node_bin(start: &Path, name: &str) -> Option<PathBuf> {
    let start = if start.is_dir() {
        start
    } else {
        start.parent()?
    };
    let exe = bin_name(name);
    start
        .ancestors()
        .map(|d| d.join("node_modules").join(".bin").join(&exe))
        .find(|p| p.is_file())
}

/// Parse JSON-with-comments (tsconfig syntax): `//` and `/* */` comments and
/// trailing commas are removed outside string literals before parsing.
pub fn parse_jsonc(text: &str) -> Option<serde_json::Value> {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    let mut in_str = false;
    while let Some(c) = chars.next() {
        if in_str {
            out.push(c);
            if c == '\\' {
                if let Some(n) = chars.next() {
                    out.push(n);
                }
            } else if c == '"' {
                in_str = false;
            }
            continue;
        }
        match c {
            '"' => {
                in_str = true;
                out.push(c);
            }
            '/' if chars.peek() == Some(&'/') => {
                for n in chars.by_ref() {
                    if n == '\n' {
                        out.push('\n');
                        break;
                    }
                }
            }
            '/' if chars.peek() == Some(&'*') => {
                chars.next();
                let mut prev = '\0';
                for n in chars.by_ref() {
                    if prev == '*' && n == '/' {
                        break;
                    }
                    prev = n;
                }
                out.push(' ');
            }
            _ => out.push(c),
        }
    }
    // Trailing commas: drop a `,` whose next non-space char closes a scope.
    let mut cleaned = String::with_capacity(out.len());
    let bytes: Vec<char> = out.chars().collect();
    let mut in_str = false;
    let mut i = 0;
    while i < bytes.len() {
        let c = bytes[i];
        if in_str {
            cleaned.push(c);
            if c == '\\' && i + 1 < bytes.len() {
                cleaned.push(bytes[i + 1]);
                i += 2;
                continue;
            }
            if c == '"' {
                in_str = false;
            }
            i += 1;
            continue;
        }
        if c == '"' {
            in_str = true;
        }
        if c == ',' {
            let next = bytes[i + 1..].iter().find(|ch| !ch.is_whitespace());
            if matches!(next, Some('}') | Some(']')) {
                i += 1;
                continue;
            }
        }
        cleaned.push(c);
        i += 1;
    }
    serde_json::from_str(&cleaned).ok()
}

/// Resolve the tsconfig that governs `file`. A "solution style" config
/// (Vite / `tsc -b` templates: `"files": []` plus `references`, no compiler
/// options of its own) configures nothing; extending it would silently fall
/// back to tsc's ES5 defaults. For those, the referenced config whose
/// `include` covers the file is used (else the first reference).
pub fn resolve_ts_project(file: &Path, root: &Path) -> Option<PathBuf> {
    let cfg = find_tsconfig(file, root)?;
    let Some(json) = std::fs::read_to_string(&cfg)
        .ok()
        .and_then(|t| parse_jsonc(&t))
    else {
        return Some(cfg);
    };
    let refs = json.get("references").and_then(|r| r.as_array());
    let solution_style = refs.is_some_and(|r| !r.is_empty())
        && json
            .get("files")
            .and_then(|f| f.as_array())
            .is_some_and(|f| f.is_empty());
    if !solution_style {
        return Some(cfg);
    }
    let dir = cfg.parent()?.to_path_buf();
    let mut first: Option<PathBuf> = None;
    for r in refs.into_iter().flatten() {
        let Some(path) = r.get("path").and_then(|p| p.as_str()) else {
            continue;
        };
        let mut target = dir.join(path);
        if target.is_dir() {
            target = target.join("tsconfig.json");
        }
        if !target.is_file() {
            continue;
        }
        let target_dir = target.parent().map(Path::to_path_buf).unwrap_or_default();
        let includes: Vec<String> = std::fs::read_to_string(&target)
            .ok()
            .and_then(|t| parse_jsonc(&t))
            .and_then(|v| v.get("include").cloned())
            .and_then(|v| v.as_array().cloned())
            .unwrap_or_default()
            .iter()
            .filter_map(|v| v.as_str().map(str::to_string))
            .collect();
        let covers = includes.iter().any(|inc| {
            // Literal directory/file prefix of the glob (before any `*`).
            let lit: PathBuf = Path::new(inc.trim_start_matches("./"))
                .components()
                .take_while(|c| !c.as_os_str().to_string_lossy().contains('*'))
                .collect();
            let base = target_dir.join(&lit);
            file.starts_with(&base) || (lit.as_os_str().is_empty() && file.starts_with(&target_dir))
        });
        if covers {
            return Some(target);
        }
        if first.is_none() {
            first = Some(target);
        }
    }
    first.or(Some(cfg))
}

/// Compiler options forced on top of the project's `tsconfig.json` for a
/// check-only run. Each neutralizes an option that is incompatible with
/// `noEmit` or would write build state (`.tsbuildinfo`) into the workspace;
/// none of them changes how the source is parsed or type-checked.
const TS_CHECK_OVERRIDES: &[(&str, bool)] = &[
    ("noEmit", true),
    ("composite", false),
    ("incremental", false),
    ("declaration", false),
    ("emitDeclarationOnly", false),
];

/// JSON for a temporary tsconfig that `extends` the project's config and
/// checks ONLY `files`.
///
/// `tsc <files>` ignores `tsconfig.json` entirely, and `tsc -p tsconfig.json`
/// type-checks the whole project (slow, and fails the edit on unrelated
/// pre-existing errors). The wrapper keeps every project compiler option
/// (target, lib, jsx, decorators, paths, types) while `files` + an empty
/// `include` restrict the root set to the edited files (imports are still
/// followed, as in any tsc run). The wrapper must live in the SAME directory
/// as the project tsconfig: `typeRoots` (`node_modules/@types`) and relative
/// paths in the project config resolve against that directory.
pub fn ts_wrapper_config_json(tsconfig: &Path, files: &[PathBuf]) -> String {
    let base = tsconfig
        .file_name()
        .map(|n| format!("./{}", n.to_string_lossy()))
        .unwrap_or_else(|| "./tsconfig.json".to_string());
    let mut opts = serde_json::Map::new();
    for (k, v) in TS_CHECK_OVERRIDES {
        opts.insert((*k).to_string(), serde_json::Value::Bool(*v));
    }
    serde_json::json!({
        "extends": base,
        "compilerOptions": opts,
        "files": files.iter().map(|f| f.to_string_lossy().to_string()).collect::<Vec<_>>(),
        "include": [],
    })
    .to_string()
}

/// `tsc` arguments used when no `tsconfig.json` applies. tsc's own defaults
/// (ES5 target/lib, no JSX) reject ordinary modern TypeScript; these are the
/// defaults a current project template would choose. Used only as the
/// FALLBACK, and reported as such.
pub fn ts_fallback_args() -> Vec<String> {
    [
        "--noEmit",
        "--target",
        "es2022",
        "--module",
        "esnext",
        "--moduleResolution",
        "node",
        "--jsx",
        "preserve",
        "--esModuleInterop",
        "--skipLibCheck",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect()
}

/// `tsc` arguments for a SYNTAX-only check of JavaScript containing JSX.
/// Node cannot parse JSX at all; tsc with `allowJs` and without `checkJs`
/// reports only syntactic diagnostics for `.js`/`.jsx` files, and
/// `noResolve` keeps it from walking into imports.
pub fn jsx_syntax_args() -> Vec<String> {
    [
        "--noEmit",
        "--allowJs",
        "--jsx",
        "preserve",
        "--target",
        "es2022",
        "--module",
        "esnext",
        "--noResolve",
        "--skipLibCheck",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect()
}

// ─────────────────────────────── JavaScript ───────────────────────────────

/// How `node --check` must treat a JavaScript file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum JsMode {
    /// Node already parses it as intended (`.mjs`, `.cjs`, a `.js` under a
    /// `package.json` `"type"`, or a `.js` with no ESM syntax).
    Native(String),
    /// ES module syntax that node would parse as CommonJS (no module
    /// `"type"`; node < 22.7 has no module detection, and an explicit
    /// `"type": "commonjs"` disables it). Checked as a module (`.mjs` copy).
    ForceModule(String),
    /// JSX (`.jsx`): node cannot parse it at all.
    Jsx,
}

/// The `"type"` field of the nearest `package.json` above `file`.
pub fn package_json_type(file: &Path, root: &Path) -> Option<(String, PathBuf)> {
    for dir in ancestor_dirs(file, root) {
        let p = dir.join("package.json");
        if !p.is_file() {
            continue;
        }
        let ty = std::fs::read_to_string(&p)
            .ok()
            .and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok())
            .and_then(|v| v.get("type").and_then(|t| t.as_str()).map(str::to_string))
            .unwrap_or_else(|| "commonjs".to_string());
        // The nearest package.json decides, even without a "type" field.
        return Some((ty, p));
    }
    None
}

/// True when `src` contains top-level ES module syntax (`import …`/`export …`
/// statements or `import.meta`). Dynamic `import(…)` is valid CommonJS and
/// does not count. Line-based on purpose: this only selects the parse goal,
/// the parser itself still decides validity.
pub fn looks_like_esm(src: &str) -> bool {
    let mut in_block_comment = false;
    for line in src.lines() {
        let t = line.trim_start();
        if in_block_comment {
            if t.contains("*/") {
                in_block_comment = false;
            }
            continue;
        }
        if t.starts_with("/*") {
            in_block_comment = !t.contains("*/");
            continue;
        }
        if t.starts_with("//") {
            continue;
        }
        if t.contains("import.meta") {
            return true;
        }
        if let Some(rest) = t.strip_prefix("import") {
            if rest.starts_with([' ', '{', '*', '"', '\'']) {
                return true;
            }
        }
        if let Some(rest) = t.strip_prefix("export") {
            if rest.starts_with([' ', '{', '*']) {
                return true;
            }
        }
    }
    false
}

/// Decide how to syntax-check the JavaScript file `file` whose content is
/// `src`.
pub fn js_mode(file: &Path, root: &Path, src: &str) -> JsMode {
    let ext = file
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    match ext.as_str() {
        "jsx" => return JsMode::Jsx,
        "mjs" => return JsMode::Native(".mjs is always an ES module".into()),
        "cjs" => return JsMode::Native(".cjs is always CommonJS".into()),
        _ => {}
    }
    let pkg = package_json_type(file, root);
    if let Some((ty, p)) = &pkg {
        if ty == "module" {
            return JsMode::Native(format!("\"type\": \"module\" in {}", p.display()));
        }
    }
    if looks_like_esm(src) {
        let why = match &pkg {
            Some((ty, p)) => format!(
                "ES module syntax in a .js file under \"type\": \"{ty}\" ({})",
                p.display()
            ),
            None => "ES module syntax in a .js file with no package.json".to_string(),
        };
        return JsMode::ForceModule(why);
    }
    JsMode::Native("CommonJS script".into())
}

/// True when a failed `node --check` looks like it choked on JSX rather than
/// on a JavaScript syntax error.
pub fn node_output_suggests_jsx(output: &str) -> bool {
    output.contains("Unexpected token '<'") || output.contains("Unexpected token <")
}

// ─────────────────────────────── C / C++ ───────────────────────────────

/// C or C++.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CLang {
    C,
    Cxx,
}

impl CLang {
    /// The compiler driver the gate invokes.
    pub fn compiler(self) -> &'static str {
        match self {
            CLang::C => "cc",
            CLang::Cxx => "c++",
        }
    }
    /// The `-x` language name (headers are parsed as source).
    pub fn x_lang(self) -> &'static str {
        match self {
            CLang::C => "c",
            CLang::Cxx => "c++",
        }
    }
}

/// Default standard when neither `compile_commands.json` nor CMake names
/// one. Compilers default to an OLDER standard (Apple clang: gnu++98/gnu++17
/// depending on version; GCC < 11: gnu++14), which rejects concepts,
/// designated initializers, `<=>` … as syntax errors. C++20 / C17 are
/// supported by every GCC ≥ 10 / clang ≥ 10. The fallback is reported as a
/// fallback in the check output.
pub const FALLBACK_CXX_STD: &str = "c++20";
/// See [`FALLBACK_CXX_STD`].
pub const FALLBACK_C_STD: &str = "c17";

/// Resolved compiler flags for a C/C++ syntax check.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CFlags {
    pub lang: CLang,
    /// Flags to pass before `-fsyntax-only -x <lang> <file>`.
    pub flags: Vec<String>,
    /// Where the flags came from.
    pub source: String,
    /// True when the standard is the built-in fallback.
    pub fallback: bool,
}

fn c_ext_lang(file: &Path) -> Option<CLang> {
    match file
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase())
        .as_deref()
    {
        Some("c") => Some(CLang::C),
        Some("cc" | "cpp" | "cxx" | "c++" | "hh" | "hpp" | "hxx") => Some(CLang::Cxx),
        _ => None, // `.h` is ambiguous
    }
}

/// Options that affect how a translation unit PARSES. Everything else in a
/// recorded compile command (`-o`, `-c`, `-M*`, plugins, codegen, warnings)
/// is dropped: the gate only asks "does this parse".
fn keep_compile_flag(arg: &str) -> Option<bool> {
    // Some(true) = flag takes the NEXT argument as its value.
    const WITH_VALUE: &[&str] = &[
        "-I",
        "-isystem",
        "-iquote",
        "-idirafter",
        "-D",
        "-U",
        "-include",
    ];
    const EXACT: &[&str] = &[
        "-fno-rtti",
        "-fno-exceptions",
        "-fms-extensions",
        "-fcoroutines",
        "-fchar8_t",
        "-fno-char8_t",
        "-fblocks",
        "-fopenmp",
        "-pthread",
    ];
    if WITH_VALUE.contains(&arg) {
        return Some(true);
    }
    if EXACT.contains(&arg) || arg.starts_with("-std=") || arg.starts_with("--std=") {
        return Some(false);
    }
    if WITH_VALUE
        .iter()
        .any(|p| arg.len() > p.len() && arg.starts_with(p))
    {
        return Some(false);
    }
    None
}

fn absolutize_path_flag(flag: &str, value: &str, dir: &Path) -> String {
    let path_flag = matches!(
        flag,
        "-I" | "-isystem" | "-iquote" | "-idirafter" | "-include"
    );
    if path_flag && !Path::new(value).is_absolute() {
        dir.join(value).to_string_lossy().to_string()
    } else {
        value.to_string()
    }
}

/// Extract the parse-relevant flags of one compile command, making relative
/// include paths absolute against the entry's `directory`.
pub fn relevant_compile_flags(args: &[String], dir: &Path) -> Vec<String> {
    let mut out = Vec::new();
    let mut i = 1; // args[0] is the compiler
    while i < args.len() {
        let a = &args[i];
        match keep_compile_flag(a) {
            Some(true) => {
                if let Some(v) = args.get(i + 1) {
                    out.push(a.clone());
                    out.push(absolutize_path_flag(a, v, dir));
                }
                i += 2;
                continue;
            }
            Some(false) => {
                // Joined form, e.g. `-Iinclude` / `-DX=1` / `-std=c++20`.
                let joined = ["-isystem", "-iquote", "-idirafter", "-include", "-I"]
                    .iter()
                    .find(|p| a.starts_with(**p) && a.len() > p.len());
                match joined {
                    Some(p) => {
                        out.push(p.to_string());
                        out.push(absolutize_path_flag(p, &a[p.len()..], dir));
                    }
                    None => out.push(a.replace("--std=", "-std=")),
                }
            }
            None => {}
        }
        i += 1;
    }
    out
}

fn entry_args(entry: &serde_json::Value) -> Option<Vec<String>> {
    if let Some(arr) = entry.get("arguments").and_then(|a| a.as_array()) {
        return Some(
            arr.iter()
                .filter_map(|v| v.as_str().map(str::to_string))
                .collect(),
        );
    }
    entry
        .get("command")
        .and_then(|c| c.as_str())
        .and_then(shlex::split)
}

fn entry_lang(args: &[String], file: &str) -> Option<CLang> {
    if let Some(l) = c_ext_lang(Path::new(file)) {
        return Some(l);
    }
    let compiler = args.first()?.rsplit('/').next()?.to_string();
    if compiler.contains("++") || compiler.contains("clang-cl") {
        Some(CLang::Cxx)
    } else if compiler.ends_with("cc") || compiler.ends_with("clang") || compiler.ends_with("gcc") {
        Some(CLang::C)
    } else {
        None
    }
}

/// Look for a `compile_commands.json` (in each ancestor, and its `build/`
/// subdirectory — CMake's conventional output location).
pub fn find_compile_commands(file: &Path, root: &Path) -> Option<PathBuf> {
    ancestor_dirs(file, root)
        .into_iter()
        .flat_map(|d| {
            [
                d.join("compile_commands.json"),
                d.join("build").join("compile_commands.json"),
            ]
        })
        .find(|p| p.is_file())
}

/// Flags for `file` from a compilation database: the file's own entry when
/// recorded, else the first entry of the same language (headers and new files
/// have no entry of their own but share the project's standard and include
/// paths).
pub fn flags_from_compile_commands(
    db: &Path,
    file: &Path,
    want: Option<CLang>,
) -> Option<(CLang, Vec<String>, String)> {
    let text = std::fs::read_to_string(db).ok()?;
    let entries: Vec<serde_json::Value> = serde_json::from_str(&text).ok()?;
    let canon = |p: &Path| std::fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf());
    let target = canon(file);
    let mut fallback: Option<(CLang, Vec<String>, String)> = None;
    for e in &entries {
        let (Some(dir), Some(f)) = (
            e.get("directory").and_then(|d| d.as_str()),
            e.get("file").and_then(|f| f.as_str()),
        ) else {
            continue;
        };
        let Some(args) = entry_args(e) else { continue };
        let dir = PathBuf::from(dir);
        let entry_file = if Path::new(f).is_absolute() {
            PathBuf::from(f)
        } else {
            dir.join(f)
        };
        let Some(lang) = entry_lang(&args, f) else {
            continue;
        };
        if canon(&entry_file) == target {
            let lang = want.unwrap_or(lang);
            return Some((
                lang,
                relevant_compile_flags(&args, &dir),
                format!("{} (entry for this file)", db.display()),
            ));
        }
        if fallback.is_none() && want.is_none_or(|w| w == lang) {
            fallback = Some((
                lang,
                relevant_compile_flags(&args, &dir),
                format!("{} (flags of {f}; no entry for this file)", db.display()),
            ));
        }
    }
    fallback
}

/// Standards declared by a CMake project: `(C++ std, C std, C++ extensions,
/// C extensions)` from `set(CMAKE_CXX_STANDARD N)` /
/// `target_compile_features(... cxx_std_N)` and the C equivalents.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CmakeStandards {
    pub cxx: Option<u32>,
    pub c: Option<u32>,
    pub cxx_extensions: bool,
    pub c_extensions: bool,
}

pub fn parse_cmake_standards(text: &str) -> CmakeStandards {
    use std::sync::OnceLock;
    static RES: OnceLock<[regex::Regex; 6]> = OnceLock::new();
    let [cxx_set, c_set, cxx_feat, c_feat, cxx_ext, c_ext] = RES.get_or_init(|| {
        [
            regex::Regex::new(r#"(?i)set\s*\(\s*CMAKE_CXX_STANDARD\s+"?(\d+)"?"#).unwrap(),
            regex::Regex::new(r#"(?i)set\s*\(\s*CMAKE_C_STANDARD\s+"?(\d+)"?"#).unwrap(),
            regex::Regex::new(r"(?i)\bcxx_std_(\d+)\b").unwrap(),
            regex::Regex::new(r"(?i)\bc_std_(\d+)\b").unwrap(),
            regex::Regex::new(r#"(?i)set\s*\(\s*CMAKE_CXX_EXTENSIONS\s+"?(\w+)"?"#).unwrap(),
            regex::Regex::new(r#"(?i)set\s*\(\s*CMAKE_C_EXTENSIONS\s+"?(\w+)"?"#).unwrap(),
        ]
    });
    // Strip `#` line comments so a commented-out standard does not count.
    let code: String = text
        .lines()
        .map(|l| l.split('#').next().unwrap_or(""))
        .collect::<Vec<_>>()
        .join("\n");
    let num = |re: &regex::Regex| {
        re.captures_iter(&code)
            .filter_map(|c| c[1].parse::<u32>().ok())
            .max_by_key(|n| normalize_std_year(*n))
    };
    // CMake's default for *_EXTENSIONS is ON (gnu++NN).
    let ext_on = |re: &regex::Regex| {
        re.captures(&code).is_none_or(|c| {
            !matches!(
                c[1].to_ascii_uppercase().as_str(),
                "OFF" | "FALSE" | "0" | "NO"
            )
        })
    };
    CmakeStandards {
        cxx: num(cxx_set).or_else(|| num(cxx_feat)),
        c: num(c_set).or_else(|| num(c_feat)),
        cxx_extensions: ext_on(cxx_ext),
        c_extensions: ext_on(c_ext),
    }
}

/// Order two-digit standard years (98 < 03 < 11 < … < 26; 90 < 99 < 11).
fn normalize_std_year(n: u32) -> u32 {
    if n >= 89 {
        1900 + n
    } else {
        2000 + n
    }
}

/// Map a CMake standard number to a `-std=` value accepted by GCC ≥ 10 and
/// clang ≥ 10 (C++23/26 use the long-supported `2b`/`2c` spellings).
pub fn cmake_std_flag(lang: CLang, n: u32, gnu: bool) -> String {
    let prefix = match (lang, gnu) {
        (CLang::Cxx, false) => "c++",
        (CLang::Cxx, true) => "gnu++",
        (CLang::C, false) => "c",
        (CLang::C, true) => "gnu",
    };
    let ver = match (lang, n) {
        (CLang::Cxx, 23) => "2b".to_string(),
        (CLang::Cxx, 26) => "2c".to_string(),
        (CLang::C, 23) => "2x".to_string(),
        (_, n) => format!("{n:02}"),
    };
    format!("-std={prefix}{ver}")
}

/// The nearest CMake-declared standards above `file`.
pub fn find_cmake_standards(file: &Path, root: &Path) -> Option<(CmakeStandards, PathBuf)> {
    for dir in ancestor_dirs(file, root) {
        let p = dir.join("CMakeLists.txt");
        if let Ok(text) = std::fs::read_to_string(&p) {
            let s = parse_cmake_standards(&text);
            if s.cxx.is_some() || s.c.is_some() {
                return Some((s, p));
            }
        }
    }
    None
}

/// Decide the language of an ambiguous `.h` header: C++ when the project is
/// evidently C++ (a C++ standard and no C standard declared, or C++ sources
/// next to the header), else C.
fn header_lang(file: &Path, cmake: Option<&CmakeStandards>) -> CLang {
    if let Some(s) = cmake {
        if s.cxx.is_some() && s.c.is_none() {
            return CLang::Cxx;
        }
    }
    let sibling_cxx = file
        .parent()
        .and_then(|d| std::fs::read_dir(d).ok())
        .into_iter()
        .flatten()
        .flatten()
        .any(|e| c_ext_lang(&e.path()) == Some(CLang::Cxx));
    if sibling_cxx {
        CLang::Cxx
    } else {
        CLang::C
    }
}

/// Resolve the compiler flags for a C/C++ syntax check of `file`.
///
/// Order: `compile_commands.json` (the project's recorded flags, filtered to
/// parse-relevant options) → `CMakeLists.txt` standard → modern fallback
/// ([`FALLBACK_CXX_STD`] / [`FALLBACK_C_STD`]).
pub fn resolve_c_flags(file: &Path, root: &Path) -> CFlags {
    let ext_lang = c_ext_lang(file);
    if let Some(db) = find_compile_commands(file, root) {
        if let Some((lang, flags, source)) = flags_from_compile_commands(&db, file, ext_lang) {
            if flags.iter().any(|f| f.starts_with("-std=")) {
                return CFlags {
                    lang,
                    flags,
                    source,
                    fallback: false,
                };
            }
            // Recorded flags without a -std: keep includes/defines, add the
            // fallback standard.
            let mut flags = flags;
            let std = match lang {
                CLang::Cxx => FALLBACK_CXX_STD,
                CLang::C => FALLBACK_C_STD,
            };
            flags.push(format!("-std={std}"));
            return CFlags {
                lang,
                flags,
                source: format!("{source}; no -std recorded, fallback -std={std}"),
                fallback: true,
            };
        }
    }
    let cmake = find_cmake_standards(file, root);
    let lang = ext_lang.unwrap_or_else(|| header_lang(file, cmake.as_ref().map(|(s, _)| s)));
    if let Some((s, path)) = &cmake {
        let std = match lang {
            CLang::Cxx => s.cxx.map(|n| cmake_std_flag(lang, n, s.cxx_extensions)),
            CLang::C => s.c.map(|n| cmake_std_flag(lang, n, s.c_extensions)),
        };
        if let Some(std) = std {
            return CFlags {
                lang,
                flags: vec![std],
                source: path.display().to_string(),
                fallback: false,
            };
        }
    }
    let std = match lang {
        CLang::Cxx => FALLBACK_CXX_STD,
        CLang::C => FALLBACK_C_STD,
    };
    CFlags {
        lang,
        flags: vec![format!("-std={std}")],
        source: format!("fallback -std={std} (no compile_commands.json or CMake standard found)"),
        fallback: true,
    }
}

// ─────────────────────────────── Python ───────────────────────────────

/// A `major.minor` Python version.
pub type PyVersion = (u32, u32);

fn parse_major_minor(s: &str) -> Option<PyVersion> {
    let s = s.trim().trim_start_matches(|c: char| !c.is_ascii_digit());
    let mut it = s.split(['.', '*']);
    let major = it.next()?.trim().parse().ok()?;
    let minor = it
        .next()
        .and_then(|m| {
            let digits: String = m.chars().take_while(|c| c.is_ascii_digit()).collect();
            digits.parse().ok()
        })
        .unwrap_or(0);
    Some((major, minor))
}

/// The MINIMUM `major.minor` a version specifier admits:
/// `>=3.10`, `~=3.11`, `==3.12.*`, `>3.9`, poetry `^3.10` / `~3.10` / `3.10`.
/// Upper bounds (`<`, `<=`) and exclusions (`!=`) are ignored.
pub fn min_python_from_specifier(spec: &str) -> Option<PyVersion> {
    spec.split(',')
        .filter_map(|clause| {
            let c = clause.trim();
            if c.starts_with('<') || c.starts_with("!=") || c.is_empty() {
                return None;
            }
            let v = c.trim_start_matches(['>', '=', '~', '^', ' ']);
            parse_major_minor(v)
        })
        .max()
}

/// A project Python pin and where it came from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PythonPin {
    pub min: PyVersion,
    pub source: String,
}

fn python_version_file(dir: &Path) -> Option<PythonPin> {
    let p = dir.join(".python-version");
    let text = std::fs::read_to_string(&p).ok()?;
    // First non-comment line; `3.12.1`, `3.12`, `pypy3.10`, `cpython-3.11`.
    let line = text
        .lines()
        .map(str::trim)
        .find(|l| !l.is_empty() && !l.starts_with('#'))?;
    let min = parse_major_minor(line)?;
    (min.0 >= 3).then(|| PythonPin {
        min,
        source: p.display().to_string(),
    })
}

fn pyproject_requires(dir: &Path) -> Option<PythonPin> {
    let p = dir.join("pyproject.toml");
    let text = std::fs::read_to_string(&p).ok()?;
    let v: toml::Value = toml::from_str(&text).ok()?;
    let spec = v
        .get("project")
        .and_then(|p| p.get("requires-python"))
        .and_then(|s| s.as_str())
        .or_else(|| {
            v.get("tool")?
                .get("poetry")?
                .get("dependencies")?
                .get("python")?
                .as_str()
        })?;
    Some(PythonPin {
        min: min_python_from_specifier(spec)?,
        source: format!("{} ({spec})", p.display()),
    })
}

fn setup_cfg_requires(dir: &Path) -> Option<PythonPin> {
    let p = dir.join("setup.cfg");
    let text = std::fs::read_to_string(&p).ok()?;
    let spec = text.lines().find_map(|l| {
        let (k, v) = l.split_once('=')?;
        (k.trim() == "python_requires").then(|| v.trim().to_string())
    })?;
    Some(PythonPin {
        min: min_python_from_specifier(&spec)?,
        source: format!("{} ({spec})", p.display()),
    })
}

/// The project's minimum Python version for `file`: nearest directory wins;
/// within a directory `.python-version` (the interpreter the project runs
/// on) beats `pyproject.toml` beats `setup.cfg`.
pub fn resolve_python_pin(file: &Path, root: &Path) -> Option<PythonPin> {
    ancestor_dirs(file, root).into_iter().find_map(|d| {
        python_version_file(&d)
            .or_else(|| pyproject_requires(&d))
            .or_else(|| setup_cfg_requires(&d))
    })
}

/// Syntax-check script run by the host interpreter. `compile()` never writes
/// bytecode (unlike `py_compile`, which dropped `__pycache__/` next to every
/// edited file); the first stdout line reports the interpreter version so the
/// gate can compare it with the project pin.
pub const PYTHON_CHECK_SCRIPT: &str = r#"import sys
print("selfware-python-version %d.%d" % sys.version_info[:2])
rc = 0
for p in sys.argv[1:]:
    try:
        with open(p, "rb") as f:
            src = f.read()
        compile(src, p, "exec", dont_inherit=True)
    except SyntaxError as e:
        rc = 1
        print('  File "%s", line %s\n    %s\nSyntaxError: %s' % (e.filename, e.lineno, (e.text or "").strip(), e.msg))
    except (OSError, ValueError) as e:
        rc = 1
        print("%s: %s" % (p, e))
sys.exit(rc)
"#;

/// Parse the version line printed by [`PYTHON_CHECK_SCRIPT`].
pub fn parse_python_check_version(stdout: &str) -> Option<PyVersion> {
    stdout
        .lines()
        .find_map(|l| l.strip_prefix("selfware-python-version "))
        .and_then(parse_major_minor)
}

// ─────────────────────────────── Java ───────────────────────────────

/// A project Java release level and where it came from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JavaRelease {
    pub release: u32,
    pub source: String,
}

/// `1.8` → 8, `17` → 17, `JavaVersion.VERSION_1_8` / `VERSION_21` handled by
/// the callers' regexes.
pub fn normalize_java_version(s: &str) -> Option<u32> {
    let s = s.trim().trim_matches(['"', '\'']);
    let s = s.strip_prefix("1.").unwrap_or(s);
    let s = s.strip_prefix("1_").unwrap_or(s);
    let digits: String = s.chars().take_while(|c| c.is_ascii_digit()).collect();
    digits.parse().ok().filter(|n| (5..100).contains(n))
}

/// Release level from a Maven `pom.xml`: `<maven.compiler.release>`,
/// compiler-plugin `<release>`, `<maven.compiler.source>`, `<source>`,
/// `<java.version>` — `${property}` references resolved within the pom.
pub fn java_release_from_pom(text: &str) -> Option<u32> {
    let tag = |name: &str| -> Option<String> {
        let re =
            regex::Regex::new(&format!(r"<{0}>\s*([^<\s]+)\s*</{0}>", regex::escape(name))).ok()?;
        re.captures(text).map(|c| c[1].to_string())
    };
    let resolve = |v: String| -> Option<u32> {
        if let Some(prop) = v.strip_prefix("${").and_then(|p| p.strip_suffix('}')) {
            return tag(prop).and_then(|x| normalize_java_version(&x));
        }
        normalize_java_version(&v)
    };
    [
        "maven.compiler.release",
        "release",
        "maven.compiler.source",
        "source",
        "java.version",
    ]
    .iter()
    .find_map(|t| tag(t).and_then(resolve))
}

/// Release level from a Gradle build script (Groovy or Kotlin DSL):
/// toolchain `JavaLanguageVersion.of(N)`, `options.release = N` /
/// `release.set(N)`, `jvmToolchain(N)`, `sourceCompatibility = …`.
pub fn java_release_from_gradle(text: &str) -> Option<u32> {
    use std::sync::OnceLock;
    static RES: OnceLock<Vec<regex::Regex>> = OnceLock::new();
    let res = RES.get_or_init(|| {
        [
            r"JavaLanguageVersion\.of\(\s*(\d+)\s*\)",
            r"release(?:\.set\(|\s*=\s*)\s*(\d+)",
            r"jvmToolchain\(\s*(\d+)\s*\)",
            r#"sourceCompatibility\s*=\s*(?:JavaVersion\.VERSION_)?['"]?([\d._]+)"#,
            r#"sourceCompatibility\s*\(\s*(?:JavaVersion\.VERSION_)?['"]?([\d._]+)"#,
        ]
        .iter()
        .map(|r| regex::Regex::new(r).unwrap())
        .collect()
    });
    let code: String = text
        .lines()
        .map(|l| l.split("//").next().unwrap_or(""))
        .collect::<Vec<_>>()
        .join("\n");
    res.iter().find_map(|re| {
        re.captures(&code)
            .and_then(|c| normalize_java_version(&c[1]))
    })
}

/// The nearest build file's release level above `file`.
pub fn resolve_java_release(file: &Path, root: &Path) -> Option<JavaRelease> {
    for dir in ancestor_dirs(file, root) {
        let pom = dir.join("pom.xml");
        if let Ok(text) = std::fs::read_to_string(&pom) {
            if let Some(release) = java_release_from_pom(&text) {
                return Some(JavaRelease {
                    release,
                    source: pom.display().to_string(),
                });
            }
        }
        for name in ["build.gradle", "build.gradle.kts"] {
            let p = dir.join(name);
            if let Ok(text) = std::fs::read_to_string(&p) {
                if let Some(release) = java_release_from_gradle(&text) {
                    return Some(JavaRelease {
                        release,
                        source: p.display().to_string(),
                    });
                }
            }
        }
    }
    None
}

/// Feature version from `javac -version` output (`javac 1.8.0_292` → 8,
/// `javac 21.0.1` → 21).
pub fn parse_javac_version(output: &str) -> Option<u32> {
    let line = output
        .lines()
        .find(|l| l.trim_start().starts_with("javac"))?;
    let v = line.trim_start().strip_prefix("javac")?.trim();
    normalize_java_version(v)
}

/// The source root of a Java file from its `package` declaration
/// (`src/main/java/com/x/A.java` with `package com.x;` → `src/main/java`),
/// so sibling classes referenced by the edited file resolve via
/// `-sourcepath` instead of failing as "cannot find symbol".
pub fn java_source_root(file: &Path, src: &str) -> Option<PathBuf> {
    let pkg = src.lines().map(str::trim).find_map(|l| {
        l.strip_prefix("package ")
            .and_then(|r| r.split(';').next())
            .map(|p| p.trim().to_string())
    });
    let mut dir = file.parent()?.to_path_buf();
    let Some(pkg) = pkg else {
        return Some(dir);
    };
    for seg in pkg.rsplit('.') {
        if dir.file_name().and_then(|n| n.to_str()) != Some(seg) {
            return None;
        }
        dir = dir.parent()?.to_path_buf();
    }
    Some(dir)
}

#[cfg(test)]
#[path = "../../tests/unit/testing/syntax_toolchain/syntax_toolchain_test.rs"]
mod tests;
