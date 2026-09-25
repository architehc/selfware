use super::*;

fn write(path: &Path, text: &str) {
    if let Some(d) = path.parent() {
        std::fs::create_dir_all(d).unwrap();
    }
    std::fs::write(path, text).unwrap();
}

// ─── ancestor bounding ───

#[test]
fn ancestor_dirs_stop_at_project_root() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("proj");
    let file = root.join("a/b/f.ts");
    write(&file, "");
    let dirs = ancestor_dirs(&file, &root);
    assert_eq!(dirs.first().unwrap(), &root.join("a/b"));
    assert_eq!(dirs.last().unwrap(), &root, "must not walk above the root");
}

#[test]
fn config_above_project_root_is_ignored() {
    let tmp = tempfile::tempdir().unwrap();
    write(&tmp.path().join("tsconfig.json"), "{}");
    let root = tmp.path().join("proj");
    let file = root.join("src/a.ts");
    write(&file, "");
    assert_eq!(find_tsconfig(&file, &root), None);
}

// ─── TypeScript ───

#[test]
fn tsconfig_nearest_wins() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    write(&root.join("tsconfig.json"), "{}");
    write(&root.join("pkg/tsconfig.json"), "{}");
    let file = root.join("pkg/src/a.ts");
    write(&file, "");
    assert_eq!(
        find_tsconfig(&file, root),
        Some(root.join("pkg/tsconfig.json"))
    );
    assert_eq!(
        resolve_ts_project(&file, root),
        Some(root.join("pkg/tsconfig.json"))
    );
}

#[test]
fn ts_solution_style_config_resolves_to_covering_reference() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    write(
        &root.join("tsconfig.json"),
        r#"{
  // Vite template: solution-style root
  "files": [],
  "references": [
    { "path": "./tsconfig.node.json" },
    { "path": "./tsconfig.app.json" }, /* trailing comma below */
  ],
}"#,
    );
    write(
        &root.join("tsconfig.app.json"),
        r#"{ "compilerOptions": { "jsx": "react-jsx", "paths": { "@/*": ["./src/*"] } }, "include": ["src"] }"#,
    );
    write(
        &root.join("tsconfig.node.json"),
        r#"{ "include": ["vite.config.ts"] }"#,
    );
    let file = root.join("src/App.tsx");
    write(&file, "");
    assert_eq!(
        resolve_ts_project(&file, root),
        Some(root.join("tsconfig.app.json"))
    );
    let vite = root.join("vite.config.ts");
    write(&vite, "");
    assert_eq!(
        resolve_ts_project(&vite, root),
        Some(root.join("tsconfig.node.json"))
    );
}

#[test]
fn parse_jsonc_keeps_comment_markers_inside_strings() {
    let v = parse_jsonc(r#"{ "a": "x // not a comment /* nor this */", /* c */ "b": [1, 2,], }"#)
        .expect("jsonc parses");
    assert_eq!(v["a"], "x // not a comment /* nor this */");
    assert_eq!(v["b"], serde_json::json!([1, 2]));
}

#[test]
fn ts_wrapper_extends_project_config_and_lists_only_edited_files() {
    let cfg = Path::new("/p/tsconfig.app.json");
    let files = vec![PathBuf::from("/p/src/a.ts")];
    let v: serde_json::Value = serde_json::from_str(&ts_wrapper_config_json(cfg, &files)).unwrap();
    assert_eq!(v["extends"], "./tsconfig.app.json");
    assert_eq!(v["files"], serde_json::json!(["/p/src/a.ts"]));
    assert_eq!(
        v["include"],
        serde_json::json!([]),
        "an inherited include would typecheck the whole project"
    );
    assert_eq!(v["compilerOptions"]["noEmit"], true);
    assert_eq!(v["compilerOptions"]["composite"], false);
    assert_eq!(v["compilerOptions"]["incremental"], false);
    // Nothing that changes parsing/type-checking is overridden.
    for k in ["target", "lib", "jsx", "experimentalDecorators", "strict"] {
        assert!(
            v["compilerOptions"].get(k).is_none(),
            "{k} must come from the project"
        );
    }
}

#[test]
fn ts_fallback_uses_modern_defaults() {
    let a = ts_fallback_args().join(" ");
    assert!(a.contains("--target es2022"));
    assert!(a.contains("--jsx preserve"));
    assert!(a.contains("--skipLibCheck"));
    assert!(a.contains("--noEmit"));
}

#[test]
fn local_tsc_is_found_above_the_file_including_hoisted_monorepo_root() {
    let tmp = tempfile::tempdir().unwrap();
    let bin = tmp.path().join("node_modules/.bin").join(bin_name("tsc"));
    write(&bin, "#!/bin/sh\n");
    let file = tmp.path().join("packages/app/src/a.ts");
    write(&file, "");
    assert_eq!(find_local_node_bin(&file, "tsc"), Some(bin));
    let other = tempfile::tempdir().unwrap();
    let f2 = other.path().join("a.ts");
    write(&f2, "");
    assert_eq!(find_local_node_bin(&f2, "no-such-bin-xyz"), None);
}

// ─── JavaScript ───

#[test]
fn esm_detection() {
    assert!(looks_like_esm("import x from 'y';\n"));
    assert!(looks_like_esm("import {a} from \"b\"\n"));
    assert!(looks_like_esm("import * as ns from 'm'\n"));
    assert!(looks_like_esm("import 'side-effect'\n"));
    assert!(looks_like_esm("export default 1;\n"));
    assert!(looks_like_esm("export { a };\n"));
    assert!(looks_like_esm("const u = import.meta.url;\n"));
    // CommonJS, dynamic import and comments are not ESM.
    assert!(!looks_like_esm(
        "const x = require('x');\nmodule.exports = x;\n"
    ));
    assert!(!looks_like_esm("const m = await import('x');\n"));
    assert!(!looks_like_esm("// import x from 'y'\n"));
    assert!(!looks_like_esm("/*\nimport x from 'y'\n*/\nconst a = 1;\n"));
    assert!(!looks_like_esm("const exported = 1; importantThing();\n"));
}

#[test]
fn js_mode_resolution() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    let esm = "import x from 'y';\n";
    let cjs = "const x = require('y');\n";

    assert!(matches!(
        js_mode(&root.join("a.mjs"), root, esm),
        JsMode::Native(_)
    ));
    assert!(matches!(
        js_mode(&root.join("a.cjs"), root, cjs),
        JsMode::Native(_)
    ));
    assert_eq!(js_mode(&root.join("a.jsx"), root, cjs), JsMode::Jsx);
    // No package.json: ESM syntax is checked as a module; CJS natively.
    assert!(matches!(
        js_mode(&root.join("a.js"), root, esm),
        JsMode::ForceModule(_)
    ));
    assert!(matches!(
        js_mode(&root.join("a.js"), root, cjs),
        JsMode::Native(_)
    ));

    // "type": "module" → node parses natively as ESM.
    write(&root.join("m/package.json"), r#"{"type":"module"}"#);
    assert!(matches!(
        js_mode(&root.join("m/a.js"), root, esm),
        JsMode::Native(_)
    ));
    // Explicit commonjs disables node's module detection → force module.
    write(&root.join("c/package.json"), r#"{"type":"commonjs"}"#);
    assert!(matches!(
        js_mode(&root.join("c/a.js"), root, esm),
        JsMode::ForceModule(_)
    ));
    assert_eq!(
        package_json_type(&root.join("c/a.js"), root).map(|(t, _)| t),
        Some("commonjs".to_string())
    );
}

#[test]
fn node_jsx_error_detection() {
    assert!(node_output_suggests_jsx(
        "a.js:1\nconst a = <div/>;\n          ^\n\nSyntaxError: Unexpected token '<'"
    ));
    assert!(!node_output_suggests_jsx(
        "SyntaxError: Unexpected token ';'"
    ));
}

// ─── C / C++ ───

#[test]
fn cmake_standards_parse() {
    let s = parse_cmake_standards(
        "cmake_minimum_required(VERSION 3.20)\n# set(CMAKE_CXX_STANDARD 11)\nset(CMAKE_CXX_STANDARD 20)\nset(CMAKE_CXX_EXTENSIONS OFF)\n",
    );
    assert_eq!(s.cxx, Some(20));
    assert_eq!(s.c, None);
    assert!(!s.cxx_extensions);
    let s = parse_cmake_standards("target_compile_features(app PUBLIC cxx_std_17 c_std_11)\n");
    assert_eq!(s.cxx, Some(17));
    assert_eq!(s.c, Some(11));
    assert!(s.cxx_extensions, "CMake's default is extensions ON");
    let s = parse_cmake_standards("set(CMAKE_C_STANDARD \"99\")\n");
    assert_eq!(s.c, Some(99));
}

#[test]
fn cmake_std_flag_mapping() {
    assert_eq!(cmake_std_flag(CLang::Cxx, 17, false), "-std=c++17");
    assert_eq!(cmake_std_flag(CLang::Cxx, 20, true), "-std=gnu++20");
    assert_eq!(cmake_std_flag(CLang::Cxx, 23, false), "-std=c++2b");
    assert_eq!(cmake_std_flag(CLang::C, 11, false), "-std=c11");
    assert_eq!(cmake_std_flag(CLang::C, 99, true), "-std=gnu99");
    assert_eq!(cmake_std_flag(CLang::C, 23, false), "-std=c2x");
}

#[test]
fn c_flags_from_cmake() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    write(
        &root.join("CMakeLists.txt"),
        "project(x CXX)\nset(CMAKE_CXX_STANDARD 17)\nset(CMAKE_CXX_EXTENSIONS OFF)\n",
    );
    let f = root.join("src/a.cpp");
    write(&f, "");
    let r = resolve_c_flags(&f, root);
    assert_eq!(r.lang, CLang::Cxx);
    assert_eq!(r.flags, vec!["-std=c++17".to_string()]);
    assert!(!r.fallback);
    // A `.h` in a C++-only CMake project is parsed as C++.
    let h = root.join("src/a.h");
    write(&h, "");
    assert_eq!(resolve_c_flags(&h, root).lang, CLang::Cxx);
}

#[test]
fn c_flags_fallback_is_modern_and_flagged() {
    let tmp = tempfile::tempdir().unwrap();
    let f = tmp.path().join("a.cc");
    write(&f, "");
    let r = resolve_c_flags(&f, tmp.path());
    assert_eq!(r.flags, vec![format!("-std={FALLBACK_CXX_STD}")]);
    assert!(r.fallback);
    let c = tmp.path().join("csrc/b.c");
    write(&c, "");
    let r = resolve_c_flags(&c, tmp.path());
    assert_eq!(r.lang, CLang::C);
    assert_eq!(r.flags, vec![format!("-std={FALLBACK_C_STD}")]);
    // A `.h` next to only C sources is C.
    let h = tmp.path().join("csrc/b.h");
    write(&h, "");
    assert_eq!(resolve_c_flags(&h, tmp.path()).lang, CLang::C);
}

#[test]
fn c_flags_from_compile_commands_reuse_parse_relevant_flags_only() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    let src = root.join("src/main.cpp");
    write(&src, "");
    let build = root.join("build");
    let db = serde_json::json!([
        {
            "directory": build.to_string_lossy(),
            "file": src.to_string_lossy(),
            "arguments": ["/usr/bin/c++", "-Iinclude", "-isystem", "/opt/x/include",
                          "-DFOO=1", "-std=gnu++2a", "-O2", "-Wall", "-Werror",
                          "-fplugin=/evil.so", "-o", "main.o", "-c", src.to_string_lossy()]
        },
        {
            "directory": build.to_string_lossy(),
            "file": "../src/other.cpp",
            "command": "c++ -I ../inc -std=c++17 -c ../src/other.cpp"
        }
    ]);
    write(&build.join("compile_commands.json"), &db.to_string());

    let r = resolve_c_flags(&src, root);
    assert!(!r.fallback);
    assert_eq!(r.lang, CLang::Cxx);
    assert_eq!(
        r.flags,
        vec![
            "-I".to_string(),
            build.join("include").to_string_lossy().to_string(),
            "-isystem".to_string(),
            "/opt/x/include".to_string(),
            "-DFOO=1".to_string(),
            "-std=gnu++2a".to_string(),
        ],
        "only parse-relevant flags; relative include dirs absolutized"
    );
    assert!(r.source.contains("entry for this file"));

    // A file without its own entry (new file / header) borrows the flags of
    // the first same-language entry.
    let newf = root.join("src/new.cpp");
    write(&newf, "");
    let r = resolve_c_flags(&newf, root);
    assert!(r.flags.contains(&"-std=gnu++2a".to_string()));
    assert!(r.source.contains("no entry for this file"));
}

#[test]
fn compile_command_string_form_is_split() {
    let dir = Path::new("/b");
    let args = shlex::split("c++ -I ../inc -std=c++17 -include pre.h -UX -c x.cpp").unwrap();
    assert_eq!(
        relevant_compile_flags(&args, dir),
        vec![
            "-I",
            "/b/../inc",
            "-std=c++17",
            "-include",
            "/b/pre.h",
            "-UX"
        ]
        .into_iter()
        .map(String::from)
        .collect::<Vec<_>>()
    );
}

// ─── Python ───

#[test]
fn python_specifier_minimum() {
    assert_eq!(min_python_from_specifier(">=3.10"), Some((3, 10)));
    assert_eq!(min_python_from_specifier(">=3.9,<4"), Some((3, 9)));
    assert_eq!(min_python_from_specifier("~=3.11"), Some((3, 11)));
    assert_eq!(min_python_from_specifier("==3.12.*"), Some((3, 12)));
    assert_eq!(min_python_from_specifier("^3.10"), Some((3, 10)));
    assert_eq!(min_python_from_specifier(">3.8, !=3.9.1"), Some((3, 8)));
    assert_eq!(min_python_from_specifier("<3.13"), None);
}

#[test]
fn python_pin_resolution_order() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    write(
        &root.join("pyproject.toml"),
        "[project]\nname='x'\nrequires-python = \">=3.10\"\n",
    );
    let f = root.join("pkg/mod.py");
    write(&f, "");
    let pin = resolve_python_pin(&f, root).unwrap();
    assert_eq!(pin.min, (3, 10));
    assert!(pin.source.contains("pyproject.toml"));

    // .python-version in the same dir beats pyproject.
    write(&root.join(".python-version"), "3.12.4\n");
    assert_eq!(resolve_python_pin(&f, root).unwrap().min, (3, 12));

    // Nearest directory wins.
    write(
        &root.join("pkg/setup.cfg"),
        "[options]\npython_requires = >=3.11\n",
    );
    let pin = resolve_python_pin(&f, root).unwrap();
    assert_eq!(pin.min, (3, 11));
    assert!(pin.source.contains("setup.cfg"));
}

#[test]
fn python_pin_poetry_and_unparseable() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    write(
        &root.join("pyproject.toml"),
        "[tool.poetry.dependencies]\npython = \"^3.11\"\n",
    );
    let f = root.join("a.py");
    write(&f, "");
    assert_eq!(resolve_python_pin(&f, root).unwrap().min, (3, 11));

    let tmp2 = tempfile::tempdir().unwrap();
    write(&tmp2.path().join(".python-version"), "system\n");
    let f2 = tmp2.path().join("a.py");
    write(&f2, "");
    assert_eq!(resolve_python_pin(&f2, tmp2.path()), None);
}

#[test]
fn python_version_line_parse() {
    assert_eq!(
        parse_python_check_version("selfware-python-version 3.9\n  File ..."),
        Some((3, 9))
    );
    assert_eq!(parse_python_check_version("garbage"), None);
    assert!(PYTHON_CHECK_SCRIPT.contains("compile("));
    assert!(
        !PYTHON_CHECK_SCRIPT.contains("py_compile"),
        "py_compile writes __pycache__ into the workspace"
    );
}

// ─── Java ───

#[test]
fn java_version_normalization() {
    assert_eq!(normalize_java_version("1.8"), Some(8));
    assert_eq!(normalize_java_version("17"), Some(17));
    assert_eq!(normalize_java_version("'11'"), Some(11));
    assert_eq!(normalize_java_version("1_8"), Some(8));
    assert_eq!(normalize_java_version("src/main"), None);
    assert_eq!(parse_javac_version("javac 1.8.0_292"), Some(8));
    assert_eq!(parse_javac_version("javac 21.0.1\n"), Some(21));
    assert_eq!(
        parse_javac_version("Picked up _JAVA_OPTIONS\njavac 26"),
        Some(26)
    );
}

#[test]
fn java_release_from_pom_variants() {
    assert_eq!(
        java_release_from_pom(
            "<properties><maven.compiler.release>17</maven.compiler.release></properties>"
        ),
        Some(17)
    );
    assert_eq!(
        java_release_from_pom("<properties><java.version>21</java.version></properties>"),
        Some(21)
    );
    assert_eq!(
        java_release_from_pom(
            "<properties><jdk>11</jdk></properties><configuration><source>${jdk}</source></configuration>"
        ),
        Some(11)
    );
    assert_eq!(
        java_release_from_pom("<maven.compiler.source>1.8</maven.compiler.source>"),
        Some(8)
    );
    assert_eq!(java_release_from_pom("<project></project>"), None);
}

#[test]
fn java_release_from_gradle_variants() {
    assert_eq!(
        java_release_from_gradle(
            "java {\n  toolchain {\n    languageVersion = JavaLanguageVersion.of(21)\n  }\n}\n"
        ),
        Some(21)
    );
    assert_eq!(
        java_release_from_gradle("sourceCompatibility = JavaVersion.VERSION_1_8\n"),
        Some(8)
    );
    assert_eq!(
        java_release_from_gradle("sourceCompatibility = '11'\n"),
        Some(11)
    );
    assert_eq!(
        java_release_from_gradle("tasks.withType<JavaCompile> { options.release.set(17) }\n"),
        Some(17)
    );
    assert_eq!(
        java_release_from_gradle("kotlin { jvmToolchain(17) }\n"),
        Some(17)
    );
    assert_eq!(
        java_release_from_gradle("// sourceCompatibility = '11'\n"),
        None,
        "commented-out settings do not count"
    );
}

#[test]
fn java_release_nearest_build_file() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    write(
        &root.join("pom.xml"),
        "<project><properties><maven.compiler.release>11</maven.compiler.release></properties></project>",
    );
    write(
        &root.join("mod/build.gradle"),
        "sourceCompatibility = '17'\n",
    );
    let f = root.join("mod/src/main/java/A.java");
    write(&f, "");
    let r = resolve_java_release(&f, root).unwrap();
    assert_eq!(r.release, 17);
    assert!(r.source.ends_with("build.gradle"));
    let g = root.join("other/A.java");
    write(&g, "");
    assert_eq!(resolve_java_release(&g, root).unwrap().release, 11);
}

#[test]
fn java_source_root_from_package() {
    let f = Path::new("/p/src/main/java/com/x/A.java");
    assert_eq!(
        java_source_root(f, "// hdr\npackage com.x;\nclass A {}"),
        Some(PathBuf::from("/p/src/main/java"))
    );
    assert_eq!(
        java_source_root(f, "package org.y;\n"),
        None,
        "a package that does not match the directory layout yields no source root"
    );
    assert_eq!(
        java_source_root(Path::new("/p/A.java"), "class A {}"),
        Some(PathBuf::from("/p"))
    );
}
