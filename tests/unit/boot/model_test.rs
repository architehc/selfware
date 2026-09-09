use super::{model_path_in, resolve_llama_server, sha256_file, verify_model_file};
use crate::boot::model::{build_server_args, MODEL_SHA256, MODEL_SIZE_BYTES};
use sha2::{Digest, Sha256};
use std::io::Write;
use std::path::PathBuf;

#[test]
fn resolve_llama_server_prefers_env_over_path() {
    let env = Some("/opt/llama/bin/llama-server".to_string());
    let path = Some(PathBuf::from("/usr/bin/llama-server"));
    assert_eq!(
        resolve_llama_server(env, path),
        Some(PathBuf::from("/opt/llama/bin/llama-server"))
    );
}

#[test]
fn resolve_llama_server_falls_back_to_path_lookup() {
    assert_eq!(
        resolve_llama_server(None, Some(PathBuf::from("/usr/bin/llama-server"))),
        Some(PathBuf::from("/usr/bin/llama-server"))
    );
    // Empty env var is treated as unset.
    assert_eq!(
        resolve_llama_server(Some(String::new()), Some(PathBuf::from("/x"))),
        Some(PathBuf::from("/x"))
    );
}

#[test]
fn resolve_llama_server_none_when_nothing_found() {
    assert_eq!(resolve_llama_server(None, None), None);
}

#[test]
fn sha256_file_matches_known_digest() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("blob");
    std::fs::write(&file, b"hello boot").unwrap();
    let expected = format!("{:x}", Sha256::digest(b"hello boot"));
    assert_eq!(sha256_file(&file).unwrap(), expected);
}

#[test]
fn verify_model_file_rejects_wrong_size_and_digest() {
    let dir = tempfile::tempdir().unwrap();
    let file = model_path_in(dir.path());
    std::fs::write(&file, b"not the model").unwrap();
    assert!(!verify_model_file(&file).unwrap());
}

/// A file with the right SIZE but wrong content must still fail on the
/// digest (the size gate is a fast path, not the check).
#[test]
fn verify_model_file_size_alone_is_not_enough() {
    let dir = tempfile::tempdir().unwrap();
    let file = model_path_in(dir.path());
    let mut f = std::fs::File::create(&file).unwrap();
    // Sparse write: one byte at the end gives the exact size cheaply.
    f.write_all(&[0u8]).unwrap();
    f.set_len(MODEL_SIZE_BYTES).unwrap();
    drop(f);
    assert_eq!(std::fs::metadata(&file).unwrap().len(), MODEL_SIZE_BYTES);
    assert!(!verify_model_file(&file).unwrap());
    // And its digest is definitely not the pinned one.
    assert_ne!(sha256_file(&file).unwrap(), MODEL_SHA256);
}

#[test]
fn build_server_args_cpu_safe_and_aliased() {
    let args = build_server_args(PathBuf::from("/tmp/m.gguf").as_path(), 8787);
    let joined = args.join(" ");
    assert!(joined.contains("-m /tmp/m.gguf"));
    assert!(joined.contains("--port 8787"));
    assert!(joined.contains("--host 127.0.0.1"));
    assert!(joined.contains("--alias boot-assistant"));
    // No GPU assumption in the recovery path.
    assert!(!args.iter().any(|a| a == "-ngl"));
}
