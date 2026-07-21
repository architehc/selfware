use super::*;

// ---- StorageEstimate tests ----

#[test]
fn test_storage_estimate_total() {
    let estimate = StorageEstimate {
        checkpoints: 1_000_000,
        logs: 200_000,
        models: 5_000_000,
        buffer: 500_000,
    };
    assert_eq!(estimate.total(), 6_700_000);
}

#[test]
fn test_storage_estimate_total_zero() {
    let estimate = StorageEstimate {
        checkpoints: 0,
        logs: 0,
        models: 0,
        buffer: 0,
    };
    assert_eq!(estimate.total(), 0);
}

#[test]
fn test_storage_estimate_total_large_values() {
    let estimate = StorageEstimate {
        checkpoints: 100_000_000_000,
        logs: 50_000_000_000,
        models: 200_000_000_000,
        buffer: 10_000_000_000,
    };
    assert_eq!(estimate.total(), 360_000_000_000);
}

#[test]
fn test_storage_estimate_clone() {
    let estimate = StorageEstimate {
        checkpoints: 42,
        logs: 7,
        models: 99,
        buffer: 13,
    };
    let cloned = estimate.clone();
    assert_eq!(cloned.checkpoints, 42);
    assert_eq!(cloned.total(), estimate.total());
}

// ---- DiskUsage tests ----

#[test]
fn test_disk_usage_default() {
    let usage = DiskUsage::default();
    assert_eq!(usage.used, 0);
    assert_eq!(usage.total, 0);
    assert_eq!(usage.available, 0);
    assert_eq!(usage.percent, 0.0);
}

#[test]
fn test_disk_usage_clone() {
    let usage = DiskUsage {
        used: 500_000_000_000,
        total: 1_000_000_000_000,
        available: 500_000_000_000,
        percent: 0.5,
    };
    let cloned = usage.clone();
    assert_eq!(cloned.used, usage.used);
    assert_eq!(cloned.total, usage.total);
    assert_eq!(cloned.available, usage.available);
    assert_eq!(cloned.percent, usage.percent);
}

// ---- DiskManager estimation tests ----

/// Helper to create a DiskManager without async, for unit testing estimation logic.
fn make_test_disk_manager(config: &DiskConfig) -> DiskManager {
    DiskManager {
        config: config.clone(),
        checkpoints_path: PathBuf::from("/tmp/test_checkpoints"),
        logs_path: PathBuf::from("/tmp/test_logs"),
        models_path: PathBuf::from("/tmp/test_models"),
        models_size_cache: std::sync::Mutex::new(None),
    }
}

#[test]
fn test_estimate_storage_needs_one_day() {
    let config = DiskConfig::default();
    let dm = make_test_disk_manager(&config);
    let estimate = dm.estimate_storage_needs(1);

    // 1 day: 500MB checkpoints + 100MB logs + 10GB models (fallback) + 1GB buffer
    assert_eq!(estimate.checkpoints, 500 * 1024 * 1024);
    assert_eq!(estimate.logs, 100 * 1024 * 1024);
    // models: fallback 10GB because /tmp/test_models does not exist
    assert_eq!(estimate.models, 10_000_000_000);
    assert_eq!(estimate.buffer, 500 * 1024 * 1024 * 2); // 2-day buffer
}

#[test]
fn test_estimate_storage_needs_thirty_days() {
    let config = DiskConfig::default();
    let dm = make_test_disk_manager(&config);
    let estimate = dm.estimate_storage_needs(30);

    assert_eq!(estimate.checkpoints, 500 * 1024 * 1024 * 30);
    assert_eq!(estimate.logs, 100 * 1024 * 1024 * 30);
    // Buffer is always 2 days regardless of run length
    assert_eq!(estimate.buffer, 500 * 1024 * 1024 * 2);
}

#[test]
fn test_estimate_storage_needs_zero_days() {
    let config = DiskConfig::default();
    let dm = make_test_disk_manager(&config);
    let estimate = dm.estimate_storage_needs(0);

    assert_eq!(estimate.checkpoints, 0);
    assert_eq!(estimate.logs, 0);
    // models and buffer are constant
    assert_eq!(estimate.models, 10_000_000_000);
    assert_eq!(estimate.buffer, 500 * 1024 * 1024 * 2);
}

#[test]
fn test_estimate_storage_total_scales_with_days() {
    let config = DiskConfig::default();
    let dm = make_test_disk_manager(&config);
    let est1 = dm.estimate_storage_needs(1);
    let est10 = dm.estimate_storage_needs(10);

    // 10-day estimate should be larger than 1-day
    assert!(est10.total() > est1.total());
    // The variable parts (checkpoints + logs) should scale by 10x
    assert_eq!(est10.checkpoints, est1.checkpoints * 10);
    assert_eq!(est10.logs, est1.logs * 10);
}

#[test]
fn test_get_models_size_returns_placeholder() {
    let config = DiskConfig::default();
    let dm = make_test_disk_manager(&config);
    // estimate_storage_needs calls get_models_size internally
    let estimate = dm.estimate_storage_needs(1);
    // Fallback 10GB because /tmp/test_models does not exist
    assert_eq!(estimate.models, 10_000_000_000);
}

#[test]
fn test_disk_config_defaults_reasonable() {
    let config = DiskConfig::default();
    assert!(config.max_usage_percent > 0.0 && config.max_usage_percent <= 1.0);
    assert!(config.maintenance_interval_seconds > 0);
    assert!(config.compress_after_days > 0);
}

// ---- get_models_size actual calculation tests ----

#[test]
fn test_get_models_size_calculates_real_directory() {
    // Create a temporary directory with known file sizes
    let temp_dir = std::env::temp_dir().join("selfware_test_models_calc");
    let _ = std::fs::remove_dir_all(&temp_dir);
    std::fs::create_dir_all(&temp_dir).unwrap();

    // Write files with known sizes
    let file_a = temp_dir.join("model_a.bin");
    let file_b = temp_dir.join("model_b.bin");
    std::fs::write(&file_a, vec![0u8; 1024]).unwrap();
    std::fs::write(&file_b, vec![0u8; 2048]).unwrap();

    // Create a subdirectory with another file
    let subdir = temp_dir.join("sub");
    std::fs::create_dir_all(&subdir).unwrap();
    std::fs::write(subdir.join("model_c.bin"), vec![0u8; 4096]).unwrap();

    let config = DiskConfig::default();
    let dm = DiskManager {
        config: config.clone(),
        checkpoints_path: PathBuf::from("/tmp/test_checkpoints"),
        logs_path: PathBuf::from("/tmp/test_logs"),
        models_path: temp_dir.clone(),
        models_size_cache: std::sync::Mutex::new(None),
    };

    let estimate = dm.estimate_storage_needs(1);
    // 1024 + 2048 + 4096 = 7168 bytes
    assert_eq!(estimate.models, 7168);

    // Cleanup
    let _ = std::fs::remove_dir_all(&temp_dir);
}

#[test]
fn test_get_models_size_caches_result() {
    let temp_dir = std::env::temp_dir().join("selfware_test_models_cache");
    let _ = std::fs::remove_dir_all(&temp_dir);
    std::fs::create_dir_all(&temp_dir).unwrap();
    std::fs::write(temp_dir.join("model.bin"), vec![0u8; 512]).unwrap();

    let config = DiskConfig::default();
    let dm = DiskManager {
        config: config.clone(),
        checkpoints_path: PathBuf::from("/tmp/test_checkpoints"),
        logs_path: PathBuf::from("/tmp/test_logs"),
        models_path: temp_dir.clone(),
        models_size_cache: std::sync::Mutex::new(None),
    };

    // First call calculates
    let estimate1 = dm.estimate_storage_needs(1);
    assert_eq!(estimate1.models, 512);

    // Add another file after the first calculation
    std::fs::write(temp_dir.join("model2.bin"), vec![0u8; 256]).unwrap();

    // Second call should return cached value (512), not 768
    let estimate2 = dm.estimate_storage_needs(1);
    assert_eq!(estimate2.models, 512);

    // Cleanup
    let _ = std::fs::remove_dir_all(&temp_dir);
}

// ---- Lazy state-dir creation tests (cwd-pollution fix) ----

/// Constructing a DiskManager must NOT create the checkpoints/logs/models
/// directories: read-only commands (`config show`, `unpack --scan`) build
/// one via the resource monitor and must leave the working dir untouched.
#[tokio::test]
async fn test_construction_does_not_create_state_dirs() {
    let root = std::env::temp_dir().join(format!("selfware_lazy_new_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);

    let checkpoints = root.join("checkpoints");
    let logs = root.join("logs");
    let models = root.join("models");

    let config = DiskConfig::default();
    let dm = DiskManager::with_paths(&config, checkpoints.clone(), logs.clone(), models.clone());
    drop(dm);

    assert!(
        !root.exists(),
        "no state dirs may be created at construction, found: {:?}",
        root
    );
    assert!(!checkpoints.exists());
    assert!(!logs.exists());
    assert!(!models.exists());
}

/// The write-side helper creates the directory on demand (and is
/// idempotent), so the first actual write still succeeds.
#[tokio::test]
async fn test_ensure_dir_creates_on_demand() {
    let root = std::env::temp_dir().join(format!("selfware_lazy_ensure_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let nested = root.join("checkpoints").join("task-1");

    assert!(!nested.exists());
    ensure_dir(&nested).await.unwrap();
    assert!(nested.is_dir());
    // Idempotent: a second write to the same dir is fine.
    ensure_dir(&nested).await.unwrap();
    assert!(nested.is_dir());

    let _ = std::fs::remove_dir_all(&root);
}

/// Maintenance is a read/clean path over the state dirs: it must tolerate
/// them being absent (lazy creation) and must not recreate them.
#[tokio::test]
async fn test_maintenance_does_not_create_missing_dirs() {
    let root = std::env::temp_dir().join(format!("selfware_lazy_maint_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);

    let checkpoints = root.join("checkpoints");
    let logs = root.join("logs");
    let models = root.join("models");

    let config = DiskConfig::default();
    let dm = DiskManager::with_paths(&config, checkpoints.clone(), logs.clone(), models.clone());

    dm.perform_maintenance()
        .await
        .expect("maintenance must tolerate missing state dirs");

    assert!(!checkpoints.exists());
    assert!(!logs.exists());
    assert!(!models.exists());
}

/// The write path still works end to end: compressing an old checkpoint
/// produces the `.chk.zst` next to it and removes the original.
#[tokio::test]
async fn test_compress_file_writes_on_demand() {
    let root = std::env::temp_dir().join(format!("selfware_lazy_compress_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let checkpoints = root.join("checkpoints");
    std::fs::create_dir_all(&checkpoints).unwrap();
    let original = checkpoints.join("old_task.json");
    std::fs::write(&original, b"{\"some\":\"checkpoint\"}").unwrap();

    let config = DiskConfig::default();
    let dm = DiskManager::with_paths(
        &config,
        checkpoints.clone(),
        root.join("logs"),
        root.join("models"),
    );

    dm.compress_file(&original).await.unwrap();

    let compressed = checkpoints.join("old_task.chk.zst");
    assert!(compressed.exists(), "compressed file must be written");
    assert!(!original.exists(), "original must be removed");
    // Untouched state dirs stay absent.
    assert!(!root.join("logs").exists());
    assert!(!root.join("models").exists());

    let _ = std::fs::remove_dir_all(&root);
}
