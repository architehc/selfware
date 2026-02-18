#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
PROJECT_DIR="${ROOT_DIR}/project/mega-workspace"

echo "[bootstrap] root: ${ROOT_DIR}"
echo "[bootstrap] project: ${PROJECT_DIR}"

if [[ -f "${PROJECT_DIR}/Cargo.toml" ]]; then
  echo "[bootstrap] project already exists; skipping scaffold"
  exit 0
fi

mkdir -p "${PROJECT_DIR}/apps" "${PROJECT_DIR}/crates"

pushd "${PROJECT_DIR}" >/dev/null

cargo new --vcs none apps/mega-cli --bin
cargo new --vcs none crates/domain --lib
cargo new --vcs none crates/storage --lib

cat > Cargo.toml <<'EOF'
[workspace]
members = [
  "apps/mega-cli",
  "crates/domain",
  "crates/storage",
]
resolver = "2"

[workspace.package]
edition = "2021"

[workspace.lints.rust]
unsafe_code = "forbid"
EOF

cat > crates/domain/src/lib.rs <<'EOF'
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum WorkStatus {
    Todo,
    InProgress,
    Done,
    Blocked,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WorkItem {
    pub id: String,
    pub title: String,
    pub status: WorkStatus,
}

impl WorkItem {
    pub fn new(id: impl Into<String>, title: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            title: title.into(),
            status: WorkStatus::Todo,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn creates_work_item() {
        let item = WorkItem::new("1", "bootstrap");
        assert_eq!(item.status, WorkStatus::Todo);
    }
}
EOF

cat > crates/domain/Cargo.toml <<'EOF'
[package]
name = "domain"
version = "0.1.0"
edition = "2021"

[dependencies]
serde = { version = "1.0", features = ["derive"] }
EOF

cat > crates/storage/src/lib.rs <<'EOF'
use std::path::Path;

pub fn storage_ready(path: &Path) -> bool {
    path.exists()
}
EOF

cat > crates/storage/Cargo.toml <<'EOF'
[package]
name = "storage"
version = "0.1.0"
edition = "2021"
EOF

cat > apps/mega-cli/src/main.rs <<'EOF'
use domain::WorkItem;
use std::path::Path;

fn main() {
    let item = WorkItem::new("bootstrap-1", "scaffold mega workspace");
    let exists = storage::storage_ready(Path::new("."));
    println!("{} | storage_ready={}", item.title, exists);
}
EOF

cat > apps/mega-cli/Cargo.toml <<'EOF'
[package]
name = "mega-cli"
version = "0.1.0"
edition = "2021"

[dependencies]
domain = { path = "../../crates/domain" }
storage = { path = "../../crates/storage" }
EOF

mkdir -p docs
cat > docs/README.md <<'EOF'
# Mega Workspace

This workspace is intentionally minimal.
Use Selfware long-run tasks to evolve architecture, tests, and reliability.
EOF

cargo check --workspace

popd >/dev/null

echo "[bootstrap] done"
