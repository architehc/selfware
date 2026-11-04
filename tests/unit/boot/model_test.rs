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
    assert!(joined.contains("-c 8192"));
    // No GPU assumption in the recovery path.
    assert!(!args.iter().any(|a| a == "-ngl"));
}

#[test]
fn bundled_llama_server_and_model_candidates_prioritize_exe_dir() {
    use super::{
        bundled_llama_server_candidates, bundled_model_candidates, find_llama_server_in_path,
        is_boot_server_endpoint, MODEL_FILE_NAME,
    };

    let exe_dir = PathBuf::from("/opt/selfware-dist");
    let boot = PathBuf::from("/home/user/.local/share/selfware/boot");
    let server_candidates = bundled_llama_server_candidates(Some(&exe_dir), &boot);
    assert!(server_candidates[0].starts_with(&exe_dir));
    assert!(server_candidates.iter().any(|p| p.starts_with(&boot)));

    let model_candidates = bundled_model_candidates(Some(&exe_dir), Some(" /custom/model.gguf "));
    assert_eq!(model_candidates[0], PathBuf::from("/custom/model.gguf"));
    assert_eq!(
        model_candidates[1],
        exe_dir.join("models").join(MODEL_FILE_NAME)
    );
    assert_eq!(model_candidates[2], exe_dir.join(MODEL_FILE_NAME));

    let temp = tempfile::tempdir().unwrap();
    let bin_name = if cfg!(windows) {
        "llama-server.exe"
    } else {
        "llama-server"
    };
    let fake_bin = temp.path().join(bin_name);
    std::fs::write(&fake_bin, b"#!/bin/sh\n").unwrap();
    assert_eq!(
        find_llama_server_in_path(Some(temp.path().as_os_str())),
        Some(fake_bin)
    );

    assert!(is_boot_server_endpoint("http://127.0.0.1:8787/v1"));
    assert!(is_boot_server_endpoint("http://localhost:8787/v1/"));
    assert!(!is_boot_server_endpoint("https://llm.selfware.design/v1"));

    let lfm3_1gb = super::find_boot_model_spec("lfm3-1.7b-gsq-1gb").expect("1GB LFM3 spec");
    assert_eq!(lfm3_1gb.file_name, "LFM3-1.7B-GSQ-1GB-IQ4_XS_RCO.gguf");
    assert_eq!(lfm3_1gb.size_bytes, 999_984_736);
    assert!(lfm3_1gb.url.contains("kaitchup/LFM3-1.7B-GSQ-GGUF"));

    let lfm3_slq = super::find_boot_model_spec("lfm3-1.7b-gsq-slq").expect("SLQ LFM3 spec");
    assert_eq!(lfm3_slq.file_name, "LFM3-1.7B-GSQ-Q6_K_SLQ.gguf");
    assert_eq!(lfm3_slq.size_bytes, 1_587_721_664);
    assert!(lfm3_slq.url.contains("kaitchup/LFM3-1.7B-GSQ-GGUF"));

    assert_eq!(super::pick_boot_model_for_memory(1024).id, "boot-assistant");
    assert_eq!(
        super::pick_boot_model_for_memory(2048).id,
        "lfm3-1.7b-gsq-1gb"
    );
    assert_eq!(
        super::pick_boot_model_for_memory(4096).id,
        "lfm3-1.7b-gsq-slq"
    );
}

#[test]
fn check_download_progress_rejects_oversized_stream() {
    use super::{check_download_complete, check_download_progress};

    let exact = check_download_progress(MODEL_SIZE_BYTES - 16, 16, MODEL_SIZE_BYTES).unwrap();
    assert_eq!(exact, MODEL_SIZE_BYTES);
    check_download_complete(exact, MODEL_SIZE_BYTES).unwrap();

    let err = check_download_progress(MODEL_SIZE_BYTES, 1, MODEL_SIZE_BYTES)
        .expect_err("stream exceeding MODEL_SIZE_BYTES must fail immediately");
    assert!(err.to_string().contains("exceeded expected size"));

    let truncated = check_download_complete(MODEL_SIZE_BYTES - 1, MODEL_SIZE_BYTES)
        .expect_err("truncated stream must fail before atomic replace");
    assert!(truncated.to_string().contains("truncated"));
}

#[cfg(unix)]
#[test]
fn verify_model_file_rejects_symlink() {
    let dir = tempfile::tempdir().unwrap();
    let target = dir.path().join("target.gguf");
    std::fs::write(&target, b"placeholder").unwrap();
    let link = model_path_in(dir.path());
    std::os::unix::fs::symlink(&target, &link).unwrap();
    assert!(!verify_model_file(&link).unwrap());
}
