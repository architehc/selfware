//! IDE file explorer and code viewer backend.
//!
//! Lists files under explicitly allowed project roots and provides versioned,
//! checkpointed document writes for the evolve IDE endpoints.

use anyhow::{Context, Result};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fs;
use std::io::Read;
use std::io::Write;
use std::path::{Component, Path, PathBuf};

use super::graph::{
    repository_relative_path_supported, repository_source_inventory, RepositoryFileClass,
};

/// Expected hash used when creating a document that must not already exist.
pub const MISSING_DOCUMENT_SHA256: &str = "missing";

/// How a file participates in the project.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FileClass {
    Production,
    Test,
    Example,
}

/// Metadata for a single entry under an allowed project root.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct FileInfo {
    pub path: String,
    pub is_dir: bool,
    pub is_symlink: bool,
    pub size: usize,
    pub classification: FileClass,
}

/// Immutable document state used for optimistic concurrency checks.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DocumentSnapshot {
    pub path: String,
    pub content: String,
    pub hash: String,
    pub lines: usize,
    pub language: String,
}

/// Metadata returned after a checked document write succeeds.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct WriteResult {
    pub path: String,
    pub previous_hash: Option<String>,
    pub hash: String,
    pub checkpoint_path: String,
    pub bytes_written: usize,
    pub lines: usize,
    pub created: bool,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct DocumentCheckpoint {
    version: u8,
    id: String,
    created_at: String,
    path: String,
    before: Option<DocumentSnapshot>,
}

#[derive(Debug, Clone)]
struct AllowedRoot {
    path: PathBuf,
    display_prefix: PathBuf,
    classification: FileClass,
}

struct ResolvedPath<'a> {
    root: &'a AllowedRoot,
    relative: PathBuf,
    full: PathBuf,
    logical: String,
}

/// File explorer and versioned document store for the IDE experience.
pub struct IdeEngine {
    project_root: PathBuf,
    allowed_roots: Vec<AllowedRoot>,
}

impl IdeEngine {
    /// Compatibility constructor for callers that provide one source root.
    ///
    /// A conventional `src`, `tests`, or `examples` path is displayed relative
    /// to its parent project. Any other path is treated as a standalone source
    /// root and remains the only writable root.
    pub fn new(src_root: impl AsRef<Path>) -> Self {
        let src_root = src_root.as_ref().to_path_buf();
        let root_name = src_root.file_name().and_then(|name| name.to_str());
        let conventional = root_name.and_then(class_for_root_name);

        let (project_root, display_prefix, classification) =
            if let Some(classification) = conventional {
                let project_root = src_root
                    .parent()
                    .filter(|path| !path.as_os_str().is_empty())
                    .unwrap_or_else(|| Path::new("."))
                    .to_path_buf();
                let display_prefix = src_root
                    .strip_prefix(&project_root)
                    .unwrap_or_else(|_| Path::new(root_name.unwrap_or("src")))
                    .to_path_buf();
                (project_root, display_prefix, classification)
            } else {
                (src_root.clone(), PathBuf::new(), FileClass::Production)
            };

        Self {
            project_root,
            allowed_roots: vec![AllowedRoot {
                path: src_root,
                display_prefix,
                classification,
            }],
        }
    }

    /// Creates an engine spanning the repository's supported source, config,
    /// and workflow text files.
    pub fn for_project(project_root: impl AsRef<Path>) -> Self {
        let project_root = project_root.as_ref().to_path_buf();
        let allowed_roots = vec![AllowedRoot {
            path: project_root.clone(),
            display_prefix: PathBuf::new(),
            classification: FileClass::Production,
        }];
        Self {
            project_root,
            allowed_roots,
        }
    }

    /// Lists entries recursively and deterministically across allowed roots.
    pub fn list_files(&self) -> Result<Vec<FileInfo>> {
        let mut files = BTreeMap::new();
        for root in &self.allowed_roots {
            if !root.path.exists() {
                continue;
            }
            reject_symlink(&root.path)?;
            if root.path.is_file() {
                let metadata = fs::symlink_metadata(&root.path)?;
                let path = root.display_prefix.to_string_lossy().replace('\\', "/");
                if !repository_relative_path_supported(&root.display_prefix)
                    || fs::read_to_string(&root.path).is_err()
                {
                    continue;
                }
                files.insert(
                    path.clone(),
                    FileInfo {
                        path,
                        is_dir: false,
                        is_symlink: metadata.file_type().is_symlink(),
                        size: metadata.len() as usize,
                        classification: root.classification,
                    },
                );
                continue;
            }
            for source in repository_source_inventory(&root.path)? {
                let relative = source.path.strip_prefix(&root.path)?;
                let mut directory = PathBuf::new();
                if let Some(parent) = relative.parent() {
                    for component in parent.components() {
                        let Component::Normal(component) = component else {
                            continue;
                        };
                        directory.push(component);
                        let logical = logical_path(root, &directory);
                        let full = root.path.join(&directory);
                        let metadata = fs::symlink_metadata(&full)?;
                        files.entry(logical.clone()).or_insert(FileInfo {
                            path: logical.clone(),
                            is_dir: true,
                            is_symlink: false,
                            size: metadata.len() as usize,
                            classification: classify_path(&logical, root.classification),
                        });
                    }
                }

                let logical = logical_path(root, relative);
                let metadata = fs::symlink_metadata(&source.path)?;
                let classification = match source.classification {
                    RepositoryFileClass::Production => root.classification,
                    RepositoryFileClass::Test => FileClass::Test,
                    RepositoryFileClass::Example => FileClass::Example,
                };
                files.insert(
                    logical.clone(),
                    FileInfo {
                        path: logical,
                        is_dir: false,
                        is_symlink: false,
                        size: metadata.len() as usize,
                        classification,
                    },
                );
            }
        }
        Ok(files.into_values().collect())
    }

    /// Reads a document together with the version metadata required for a
    /// subsequent checked write.
    pub fn read_document(&self, path: &str) -> Result<DocumentSnapshot> {
        let resolved = self.resolve_path(path)?;
        self.snapshot_if_exists(&resolved)?
            .ok_or_else(|| anyhow::anyhow!("document does not exist: {}", resolved.logical))
    }

    /// Read a document without allocating more than `max_bytes` of source.
    pub fn read_document_limited(&self, path: &str, max_bytes: usize) -> Result<DocumentSnapshot> {
        let resolved = self.resolve_path(path)?;
        self.snapshot_if_exists_limited(&resolved, max_bytes)?
            .ok_or_else(|| anyhow::anyhow!("document does not exist: {}", resolved.logical))
    }

    /// Compatibility reader returning only document content.
    pub fn read_file(&self, path: &str) -> Result<String> {
        Ok(self.read_document(path)?.content)
    }

    /// Atomically replaces `content` after verifying that the current document
    /// matches `expected_hash`. This prevents stale writes from cooperating API
    /// clients. External filesystem writers do not participate in the lock and
    /// can still race the final platform rename. Use
    /// [`MISSING_DOCUMENT_SHA256`] to create a file only when it does not
    /// already exist at the final pre-rename check.
    pub fn write_file_checked(
        &self,
        path: &str,
        content: &str,
        expected_hash: &str,
    ) -> Result<WriteResult> {
        let resolved = self.resolve_path(path)?;
        self.ensure_root_exists(resolved.root)?;
        self.reject_symlink_components(&resolved)?;

        let before = self.snapshot_if_exists(&resolved)?;
        verify_expected_hash(&resolved.logical, before.as_ref(), expected_hash)?;

        let checkpoint_path = self.write_checkpoint(&resolved.logical, before.clone())?;
        let parent = resolved
            .full
            .parent()
            .ok_or_else(|| anyhow::anyhow!("document has no parent directory"))?;
        let parent_relative = resolved.relative.parent().unwrap_or_else(|| Path::new(""));
        if resolved.root.path.is_dir() {
            ensure_directory_chain(&resolved.root.path, parent_relative)?;
        }
        self.reject_symlink_components(&resolved)?;

        let mut temporary = tempfile::NamedTempFile::new_in(parent)
            .with_context(|| format!("failed to create temporary file in {}", parent.display()))?;
        temporary.write_all(content.as_bytes())?;
        temporary.flush()?;
        if let Some(snapshot) = &before {
            let permissions = fs::metadata(&resolved.full)
                .with_context(|| format!("failed to read permissions for {}", snapshot.path))?
                .permissions();
            temporary.as_file().set_permissions(permissions)?;
        }
        temporary.as_file().sync_all()?;

        // Recheck immediately before replacement so an edit made while the
        // checkpoint/temp file was being written is rejected as stale.
        self.reject_symlink_components(&resolved)?;
        let latest = self.snapshot_if_exists(&resolved)?;
        verify_expected_hash(&resolved.logical, latest.as_ref(), expected_hash)?;

        temporary.persist(&resolved.full).map_err(|error| {
            anyhow::anyhow!(
                "failed to atomically replace {}: {}",
                resolved.logical,
                error.error
            )
        })?;
        sync_directory(parent)?;

        let hash = content_sha256(content);
        Ok(WriteResult {
            path: resolved.logical,
            previous_hash: before.as_ref().map(|snapshot| snapshot.hash.clone()),
            hash,
            checkpoint_path: checkpoint_path.to_string_lossy().to_string(),
            bytes_written: content.len(),
            lines: line_count(content),
            created: before.is_none(),
        })
    }

    /// Compatibility writer. It still uses the checked, checkpointed, atomic
    /// path, but callers that need cross-request stale-write protection must use
    /// [`Self::write_file_checked`] with a hash from [`Self::read_document`].
    pub fn write_file(&self, path: &str, content: &str) -> Result<()> {
        let resolved = self.resolve_path(path)?;
        self.ensure_root_exists(resolved.root)?;
        let expected = self
            .snapshot_if_exists(&resolved)?
            .map(|snapshot| snapshot.hash)
            .unwrap_or_else(|| MISSING_DOCUMENT_SHA256.to_string());
        self.write_file_checked(path, content, &expected)
            .map(|_| ())
    }

    fn resolve_path(&self, requested: &str) -> Result<ResolvedPath<'_>> {
        let requested = Path::new(requested);
        validate_relative_path(requested)?;

        for root in &self.allowed_roots {
            if root.path.is_file() && requested == root.display_prefix {
                return validate_supported_document(ResolvedPath {
                    root,
                    relative: PathBuf::new(),
                    full: root.path.clone(),
                    logical: root.display_prefix.to_string_lossy().replace('\\', "/"),
                });
            }
            if !root.display_prefix.as_os_str().is_empty()
                && root.path.is_dir()
                && requested.starts_with(&root.display_prefix)
            {
                let relative = requested.strip_prefix(&root.display_prefix)?;
                validate_relative_path(relative)?;
                return validate_supported_document(ResolvedPath {
                    root,
                    relative: relative.to_path_buf(),
                    full: root.path.join(relative),
                    logical: logical_path(root, relative),
                });
            }
        }

        if self.allowed_roots.len() == 1 {
            let root = &self.allowed_roots[0];
            return validate_supported_document(ResolvedPath {
                root,
                relative: requested.to_path_buf(),
                full: root.path.join(requested),
                logical: logical_path(root, requested),
            });
        }

        anyhow::bail!("path is outside the allowed project source set")
    }

    fn ensure_root_exists(&self, root: &AllowedRoot) -> Result<()> {
        let project_metadata = fs::symlink_metadata(&self.project_root).with_context(|| {
            format!(
                "project root does not exist: {}",
                self.project_root.display()
            )
        })?;
        if project_metadata.file_type().is_symlink() || !project_metadata.is_dir() {
            anyhow::bail!("project root must be a real directory");
        }
        if root.path.is_file() {
            reject_symlink(&root.path)?;
            return Ok(());
        }
        let relative = match root.path.strip_prefix(&self.project_root) {
            Ok(relative) => relative,
            Err(_) if self.project_root == Path::new(".") && root.path.is_relative() => {
                root.path.as_path()
            }
            Err(_) => {
                anyhow::bail!("allowed root escapes project root: {}", root.path.display())
            }
        };
        ensure_directory_chain(&self.project_root, relative)
    }

    fn reject_symlink_components(&self, resolved: &ResolvedPath<'_>) -> Result<()> {
        reject_symlink(&resolved.root.path)?;
        let mut current = resolved.root.path.clone();
        for component in resolved.relative.components() {
            let Component::Normal(component) = component else {
                anyhow::bail!("path traversal rejected");
            };
            current.push(component);
            match fs::symlink_metadata(&current) {
                Ok(metadata) if metadata.file_type().is_symlink() => {
                    anyhow::bail!("symlink path rejected: {}", current.display());
                }
                Ok(_) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => break,
                Err(error) => return Err(error.into()),
            }
        }
        Ok(())
    }

    fn snapshot_if_exists(&self, resolved: &ResolvedPath<'_>) -> Result<Option<DocumentSnapshot>> {
        self.snapshot_if_exists_with_limit(resolved, None)
    }

    fn snapshot_if_exists_limited(
        &self,
        resolved: &ResolvedPath<'_>,
        max_bytes: usize,
    ) -> Result<Option<DocumentSnapshot>> {
        self.snapshot_if_exists_with_limit(resolved, Some(max_bytes))
    }

    fn snapshot_if_exists_with_limit(
        &self,
        resolved: &ResolvedPath<'_>,
        max_bytes: Option<usize>,
    ) -> Result<Option<DocumentSnapshot>> {
        self.reject_symlink_components(resolved)?;
        let metadata = match fs::symlink_metadata(&resolved.full) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(error.into()),
        };
        if metadata.file_type().is_symlink() {
            anyhow::bail!("symlink path rejected: {}", resolved.full.display());
        }
        if !metadata.is_file() {
            anyhow::bail!("document is not a regular file: {}", resolved.logical);
        }
        if let Some(limit) = max_bytes {
            if metadata.len() > limit as u64 {
                anyhow::bail!("document exceeds the {limit}-byte read limit");
            }
        }
        let content = if let Some(limit) = max_bytes {
            let file = fs::File::open(&resolved.full)
                .with_context(|| format!("failed to open {}", resolved.logical))?;
            let mut content = String::new();
            file.take(limit.saturating_add(1) as u64)
                .read_to_string(&mut content)
                .with_context(|| format!("failed to read {}", resolved.logical))?;
            if content.len() > limit {
                anyhow::bail!("document exceeds the {limit}-byte read limit");
            }
            content
        } else {
            fs::read_to_string(&resolved.full)
                .with_context(|| format!("failed to read {}", resolved.logical))?
        };
        Ok(Some(DocumentSnapshot {
            path: resolved.logical.clone(),
            hash: content_sha256(&content),
            lines: line_count(&content),
            language: language_for(&resolved.full).to_string(),
            content,
        }))
    }

    fn write_checkpoint(
        &self,
        logical_path: &str,
        before: Option<DocumentSnapshot>,
    ) -> Result<PathBuf> {
        let checkpoint_dir = self
            .project_root
            .join(".selfware")
            .join("evolve-checkpoints");
        ensure_directory_chain(
            &self.project_root,
            Path::new(".selfware/evolve-checkpoints"),
        )?;
        reject_symlink(&checkpoint_dir)?;
        make_private(&checkpoint_dir)?;

        let id = uuid::Uuid::new_v4().to_string();
        let checkpoint = DocumentCheckpoint {
            version: 1,
            id: id.clone(),
            created_at: chrono::Utc::now().to_rfc3339(),
            path: logical_path.to_string(),
            before,
        };
        let checkpoint_path = checkpoint_dir.join(format!("{id}.json"));
        let bytes = serde_json::to_vec_pretty(&checkpoint)?;
        let mut temporary = tempfile::NamedTempFile::new_in(&checkpoint_dir)?;
        temporary.write_all(&bytes)?;
        temporary.write_all(b"\n")?;
        temporary.flush()?;
        temporary.as_file().sync_all()?;
        temporary.persist(&checkpoint_path).map_err(|error| {
            anyhow::anyhow!(
                "failed to persist checkpoint {}: {}",
                checkpoint_path.display(),
                error.error
            )
        })?;
        sync_directory(&checkpoint_dir)?;
        Ok(checkpoint_path)
    }

    /// Read a path emitted by the graph builder. Graph paths may be absolute,
    /// but only when they remain beneath this engine's exact project root.
    pub fn graph_logical_path(&self, graph_path: &str) -> Result<String> {
        let path = Path::new(graph_path);
        if !path.is_absolute() {
            return Ok(self.resolve_path(graph_path)?.logical);
        }
        let relative = path.strip_prefix(&self.project_root).with_context(|| {
            format!(
                "graph path escapes project root: {}",
                path.to_string_lossy()
            )
        })?;
        Ok(self.resolve_path(&relative.to_string_lossy())?.logical)
    }

    pub fn read_graph_document(&self, graph_path: &str) -> Result<DocumentSnapshot> {
        self.read_document(&self.graph_logical_path(graph_path)?)
    }
}

fn class_for_root_name(name: &str) -> Option<FileClass> {
    match name {
        "src" | "scripts" | "workflows" => Some(FileClass::Production),
        "tests" | "system_tests" | "benches" | "fuzz" => Some(FileClass::Test),
        "examples" => Some(FileClass::Example),
        _ => None,
    }
}

fn classify_path(path: &str, fallback: FileClass) -> FileClass {
    let normalized = path.replace('\\', "/");
    let file_name = normalized.rsplit('/').next().unwrap_or_default();
    if normalized.starts_with("tests/")
        || normalized == "tests"
        || normalized.contains("/tests/")
        || normalized.starts_with("system_tests/")
        || normalized == "system_tests"
        || normalized.starts_with("benches/")
        || normalized == "benches"
        || normalized.starts_with("fuzz/")
        || normalized == "fuzz"
        || normalized.contains("/__tests__/")
        || normalized.contains("/test/")
        || file_name.ends_with("_test.rs")
        || file_name.ends_with("_tests.rs")
        || file_name.contains(".test.")
        || file_name.contains(".spec.")
        || (file_name.ends_with(".py") && file_name.starts_with("test_"))
        || file_name.ends_with("_test.go")
        || file_name.ends_with("_spec.rb")
    {
        FileClass::Test
    } else if normalized.starts_with("examples/")
        || normalized == "examples"
        || normalized.contains("/examples/")
    {
        FileClass::Example
    } else {
        fallback
    }
}

fn logical_path(root: &AllowedRoot, relative: &Path) -> String {
    root.display_prefix
        .join(relative)
        .to_string_lossy()
        .replace('\\', "/")
}

fn validate_supported_document(resolved: ResolvedPath<'_>) -> Result<ResolvedPath<'_>> {
    if !repository_relative_path_supported(Path::new(&resolved.logical)) {
        anyhow::bail!(
            "document type or location is outside the supported repository source set: {}",
            resolved.logical
        );
    }
    Ok(resolved)
}

fn validate_relative_path(path: &Path) -> Result<()> {
    if path.as_os_str().is_empty() {
        anyhow::bail!("empty document path rejected");
    }
    for component in path.components() {
        if !matches!(component, Component::Normal(_)) {
            anyhow::bail!("path traversal rejected");
        }
    }
    Ok(())
}

fn reject_symlink(path: &Path) -> Result<()> {
    let metadata = fs::symlink_metadata(path)?;
    if metadata.file_type().is_symlink() {
        anyhow::bail!("symlink path rejected: {}", path.display());
    }
    Ok(())
}

fn ensure_directory_chain(base: &Path, relative: &Path) -> Result<()> {
    let base_metadata = fs::symlink_metadata(base)
        .with_context(|| format!("base directory does not exist: {}", base.display()))?;
    if base_metadata.file_type().is_symlink() || !base_metadata.is_dir() {
        anyhow::bail!("base path must be a real directory: {}", base.display());
    }

    let mut current = base.to_path_buf();
    for component in relative.components() {
        let Component::Normal(component) = component else {
            anyhow::bail!("path traversal rejected");
        };
        current.push(component);
        match fs::symlink_metadata(&current) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                anyhow::bail!("symlink path rejected: {}", current.display());
            }
            Ok(metadata) if !metadata.is_dir() => {
                anyhow::bail!("path component is not a directory: {}", current.display());
            }
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                fs::create_dir(&current)
                    .with_context(|| format!("failed to create directory {}", current.display()))?;
                reject_symlink(&current)?;
            }
            Err(error) => return Err(error.into()),
        }
    }
    Ok(())
}

fn verify_expected_hash(
    path: &str,
    current: Option<&DocumentSnapshot>,
    expected_hash: &str,
) -> Result<()> {
    match current {
        Some(snapshot) if snapshot.hash == expected_hash => Ok(()),
        None if expected_hash == MISSING_DOCUMENT_SHA256 => Ok(()),
        Some(snapshot) => anyhow::bail!(
            "stale write rejected for {path}: expected {expected_hash}, current {}",
            snapshot.hash
        ),
        None => anyhow::bail!(
            "stale write rejected for {path}: expected {expected_hash}, document is missing"
        ),
    }
}

fn content_sha256(content: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(content.as_bytes());
    hex::encode(hasher.finalize())
}

fn line_count(content: &str) -> usize {
    content.lines().count()
}

fn language_for(path: &Path) -> &'static str {
    match path.extension().and_then(|extension| extension.to_str()) {
        Some("rs") => "rust",
        Some("toml") => "toml",
        Some("md") => "markdown",
        Some("js") => "javascript",
        Some("ts") => "typescript",
        Some("html") => "html",
        Some("css") => "css",
        Some("json") => "json",
        Some("yaml" | "yml") => "yaml",
        Some("py") => "python",
        Some("sh" | "bash" | "zsh") => "shell",
        Some("go") => "go",
        Some("c" | "h") => "c",
        Some("cc" | "cpp" | "hpp") => "cpp",
        _ => "plaintext",
    }
}

#[cfg(unix)]
fn make_private(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o700))?;
    Ok(())
}

#[cfg(not(unix))]
fn make_private(_path: &Path) -> Result<()> {
    Ok(())
}

#[cfg(unix)]
fn sync_directory(path: &Path) -> Result<()> {
    fs::File::open(path)?.sync_all()?;
    Ok(())
}

#[cfg(not(unix))]
fn sync_directory(_path: &Path) -> Result<()> {
    Ok(())
}
