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
    assert!(is_boot_server_endpoint("http://127.0.0.1:8788/v1"));
    assert!(is_boot_server_endpoint("http://127.0.0.1:8797/v1"));
    assert!(!is_boot_server_endpoint("http://127.0.0.1:8798/v1"));
    assert!(!is_boot_server_endpoint("https://llm.selfware.design/v1"));

    assert!(super::is_llama_health_json(r#"{"status":"ok"}"#));
    assert!(!super::is_llama_health_json(
        "<!doctype html><html><body>SPA</body></html>"
    ));
    assert!(!super::is_llama_health_json(r#"{"status":"loading"}"#));

    let mem = super::measure_memory_snapshot();
    assert!(mem.selfware_rss_mb > 0, "selfware_rss_mb must be measured");
    assert!(mem.system_total_mb > 0, "system_total_mb must be measured");
    assert!(mem.summary_line().contains("MiB RSS"));

    let lfm3_1gb = super::find_boot_model_spec("lfm3-1.7b-gsq-1gb").expect("1GB LFM3 spec");
    assert_eq!(lfm3_1gb.file_name, "LFM3-1.7B-GSQ-1GB-IQ4_XS_RCO.gguf");
    assert_eq!(lfm3_1gb.size_bytes, 999_984_736);
    assert!(lfm3_1gb.url.contains("kaitchup/LFM3-1.7B-GSQ-GGUF"));
    assert_eq!(super::shipped_boot_model_spec().id, "boot-assistant");
    assert_eq!(
        super::shipped_boot_model_spec().file_name,
        "functiongemma-270m-it-UD-Q6_K_XL.gguf"
    );
    assert_eq!(super::SHIPPED_BOOT_MODEL_ID, "boot-assistant");
    assert_eq!(super::PRIMARY_LOCAL_CODING_MODEL_ID, "lfm3-1.7b-gsq-1gb");

    let fg = super::find_boot_model_spec("functiongemma-270m-it").expect("FunctionGemma 270M spec");
    assert_eq!(fg.id, "boot-assistant");
    assert_eq!(fg.file_name, "functiongemma-270m-it-UD-Q6_K_XL.gguf");
    assert_eq!(fg.size_bytes, 286_162_144);
    assert!(fg.url.contains("unsloth/functiongemma-270m-it-GGUF"));
    assert_eq!(
        super::resolve_boot_model_context(Some("functiongemma-270m-it"), None),
        32_768
    );

    let ready_high = super::evaluate_model_resource_readiness(lfm3_1gb, 65_536, true, 4096, 8192);
    assert!(ready_high.is_ready());
    assert_eq!(ready_high.recommended_model_id, "lfm3-1.7b-gsq-256k");
    assert!(!ready_high.tips.is_empty());

    let low_ram = super::evaluate_model_resource_readiness(lfm3_1gb, 131_072, false, 800, 2048);
    assert!(!low_ram.ram_ok);
    assert!(low_ram.disk_ok);
    assert_eq!(low_ram.recommended_model_id, "boot-assistant");
    assert!(low_ram
        .tips
        .iter()
        .any(|t| t.contains("functiongemma-270m-it")));

    let low_disk = super::evaluate_model_resource_readiness(lfm3_1gb, 65_536, false, 4096, 500);
    assert!(low_disk.ram_ok);
    assert!(!low_disk.disk_ok);
    assert!(low_disk
        .tips
        .iter()
        .any(|t| t.contains("Disk space low") && t.contains("SELFWARE_BOOT_MODEL")));

    let banner = super::format_download_preflight_banner(
        fg,
        PathBuf::from("/tmp/functiongemma-270m-it-UD-Q6_K_XL.gguf").as_path(),
        &ready_high,
    );
    assert!(banner.contains("Selfware Local Model Download"));
    assert!(banner.contains("functiongemma-270m-it-UD-Q6_K_XL.gguf"));
    assert!(banner.contains("Host RAM:"));
    assert!(banner.contains("Host Disk:"));

    let bar_half = super::format_download_progress_bar(143_081_072, 286_162_144, 5.0);
    assert!(bar_half.contains("50.0%"));
    assert!(bar_half.contains("MiB/s"));
    assert!(bar_half.contains("ETA"));
    let bar_done = super::format_download_progress_bar(286_162_144, 286_162_144, 8.0);
    assert!(bar_done.contains("100.0%"));
    assert!(bar_done.contains("done"));

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
fn lfm3_context_windows_64k_128k_256k_and_platform_runtimes_resolve() {
    use super::{
        build_server_args_with_context, find_boot_model_spec, llama_runtime_spec_for,
        resolve_boot_model_context, LFM3_CTX_128K, LFM3_CTX_256K, LFM3_CTX_64K,
        LFM3_SUPPORTED_CONTEXTS,
    };

    assert_eq!(LFM3_SUPPORTED_CONTEXTS, &[65_536, 131_072, 262_144]);

    // Every target platform (macOS arm64/x64, Linux x64, Windows x64) has a pinned b12110 runtime
    for (os, arch, expected_archive) in [
        ("macos", "aarch64", "llama-b12110-bin-macos-arm64.tar.gz"),
        ("macos", "x86_64", "llama-b12110-bin-macos-x64.tar.gz"),
        ("linux", "x86_64", "llama-b12110-bin-ubuntu-x64.tar.gz"),
        ("windows", "x86_64", "llama-b12110-bin-win-cpu-x64.zip"),
    ] {
        let spec = llama_runtime_spec_for(os, arch)
            .unwrap_or_else(|| panic!("missing runtime spec for {os}/{arch}"));
        assert_eq!(spec.archive_name, expected_archive);
        assert_eq!(spec.sha256.len(), 64);
        assert!(spec.size_bytes > 10_000_000);
    }

    // Context-tagged aliases map to the 1GB LFM3 spec and their respective 64k/128k/256k windows
    for (alias, expected_ctx) in [
        ("lfm3-1.7b-gsq-64k", LFM3_CTX_64K),
        ("lfm3-1.7b-gsq-128k", LFM3_CTX_128K),
        ("lfm3-1.7b-gsq-256k", LFM3_CTX_256K),
        ("lfm3-1.7b-gsq-1gb", LFM3_CTX_64K),
    ] {
        let spec = find_boot_model_spec(alias).expect(alias);
        assert_eq!(spec.id, "lfm3-1.7b-gsq-1gb");
        assert_eq!(resolve_boot_model_context(Some(alias), None), expected_ctx);
    }

    // Explicit config context_length overrides default for lfm3-1.7b-gsq-1gb / slq
    assert_eq!(
        resolve_boot_model_context(Some("lfm3-1.7b-gsq-1gb"), Some(LFM3_CTX_128K)),
        LFM3_CTX_128K
    );
    assert_eq!(
        resolve_boot_model_context(Some("lfm3-1.7b-gsq-slq"), Some(LFM3_CTX_256K)),
        LFM3_CTX_256K
    );

    // Server args for 64k and 128k use q8_0 KV cache + -np 1 --no-repack; 256k uses q4_0 KV cache (<2 GiB RSS)
    let model_path = PathBuf::from("/tmp/LFM3-1.7B-GSQ-1GB-IQ4_XS_RCO.gguf");
    let args_64k =
        build_server_args_with_context(&model_path, "lfm3-1.7b-gsq-64k", 8787, LFM3_CTX_64K);
    let joined_64k = args_64k.join(" ");
    assert!(joined_64k.contains("-c 65536"));
    assert!(joined_64k.contains("-np 1 --no-repack -fa on"));
    assert!(joined_64k.contains("-ctk q8_0 -ctv q8_0"));

    let args_128k =
        build_server_args_with_context(&model_path, "lfm3-1.7b-gsq-128k", 8787, LFM3_CTX_128K);
    let joined_128k = args_128k.join(" ");
    assert!(joined_128k.contains("-c 131072"));
    assert!(joined_128k.contains("-ctk q8_0 -ctv q8_0"));

    let args_256k =
        build_server_args_with_context(&model_path, "lfm3-1.7b-gsq-256k", 8787, LFM3_CTX_256K);
    let joined_256k = args_256k.join(" ");
    assert!(joined_256k.contains("-c 262144"));
    assert!(joined_256k.contains("-np 1 --no-repack -fa on"));
    assert!(joined_256k.contains("-ctk q4_0 -ctv q4_0"));
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

#[tokio::test]
async fn non_llama_html_server_is_rejected_by_is_boot_server_healthy() {
    use std::io::Read;
    use std::net::TcpListener;

    // 1. Spawn a mock local web server (e.g. a dev SPA on localhost) returning 200 OK text/html
    let html_listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let html_port = html_listener.local_addr().unwrap().port();
    std::thread::spawn(move || {
        if let Ok((mut stream, _)) = html_listener.accept() {
            let mut buf = [0u8; 512];
            let _ = stream.read(&mut buf);
            let body = "<!doctype html><html><body>not llama-server</body></html>";
            let resp = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            let _ = stream.write_all(resp.as_bytes());
        }
    });
    assert!(
        !super::is_boot_server_healthy(html_port).await,
        "HTML SPA returning 200 OK must not be mistaken for a healthy llama-server"
    );

    // 2. Spawn a mock llama-server returning 200 OK with JSON `{"status":"ok"}`
    let llama_listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let llama_port = llama_listener.local_addr().unwrap().port();
    std::thread::spawn(move || {
        if let Ok((mut stream, _)) = llama_listener.accept() {
            let mut buf = [0u8; 512];
            let _ = stream.read(&mut buf);
            let body = r#"{"status":"ok"}"#;
            let resp = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            let _ = stream.write_all(resp.as_bytes());
        }
    });
    assert!(
        super::is_boot_server_healthy(llama_port).await,
        "Real llama-server JSON health response must pass"
    );
}

/// Static source-level guard (AGENTS.md Rule 5): every file in `src/` that
/// matches on `crossterm`'s `Event::Key(` MUST explicitly filter `KeyEventKind`
/// (`KeyEventKind::Press` / `KeyEventKind::Repeat` / `KeyEventKind::Release` or
/// `should_accept_wizard_key_kind`) so Windows Console `KeyEventKind::Release`
/// events can never cause double-input regressions without requiring a Windows
/// machine to catch them.
#[test]
fn all_crossterm_key_event_handlers_filter_windows_key_release() {
    fn walk_rs_files(dir: &std::path::Path, out: &mut Vec<PathBuf>) {
        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    walk_rs_files(&path, out);
                } else if path.extension().and_then(|e| e.to_str()) == Some("rs") {
                    out.push(path);
                }
            }
        }
    }

    let src_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut files = Vec::new();
    walk_rs_files(&src_dir, &mut files);
    assert!(!files.is_empty(), "src/ must contain Rust files");

    let mut checked = 0usize;
    let mut violations = Vec::new();
    for file in files {
        let content = std::fs::read_to_string(&file).unwrap_or_default();
        if content.contains("Event::Key(") || content.contains("CrosstermEvent::Key(") {
            checked += 1;
            let has_kind_guard = content.contains("KeyEventKind::")
                || content.contains("should_accept_wizard_key_kind");
            if !has_kind_guard {
                violations.push(file.display().to_string());
            }
        }
    }
    assert!(
        checked >= 4,
        "expected at least 4 Event::Key handlers in src/, found {checked}"
    );
    assert!(
        violations.is_empty(),
        "Found Event::Key handlers without Windows KeyEventKind filtering (causes double input on Windows): {violations:?}"
    );
}
