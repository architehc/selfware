use std::path::Path;

pub fn storage_ready(path: &Path) -> bool {
    path.exists()
}
