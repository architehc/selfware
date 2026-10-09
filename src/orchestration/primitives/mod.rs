//! Read-only, artifact-scoped model orchestration.
//!
//! The primitive harness deliberately lives beside, rather than inside, the
//! normal agent loop. A primitive receives one bounded, sanitized artifact
//! snapshot and can only return a strict data object. It never receives a
//! [`ToolRegistry`](crate::tools::ToolRegistry), a shell, filesystem mutation
//! authority, or an execution capability.

mod activation;
mod broker;
mod runtime;
mod safe_read;
mod types;

pub(crate) use runtime::run_artifact_harness;
pub(crate) use types::ArtifactRunReport;
