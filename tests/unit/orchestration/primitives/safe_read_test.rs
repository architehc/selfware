use super::*;

fn permissive_safety() -> SafetyConfig {
    SafetyConfig {
        allowed_paths: vec!["/**".to_string()],
        ..SafetyConfig::default()
    }
}

#[cfg(unix)]
#[tokio::test]
async fn regular_file_below_root_is_read_from_its_pinned_descriptor() {
    let temp = tempfile::Builder::new()
        .prefix(".primitive-safe-read-")
        .tempdir_in(".")
        .unwrap();
    let root = std::fs::canonicalize(temp.path()).unwrap();
    std::fs::create_dir(root.join("src")).unwrap();
    std::fs::write(root.join("src/lib.rs"), b"pub fn safe() {}\n").unwrap();

    let (text, bytes) =
        read_relative_regular(&root, Path::new("src/lib.rs"), &permissive_safety(), 1024)
            .await
            .unwrap();

    assert_eq!(text, "pub fn safe() {}\n");
    assert_eq!(bytes, b"pub fn safe() {}\n");
}

#[cfg(unix)]
#[tokio::test]
async fn regular_file_below_uncanonicalized_tempdir_root_is_read() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    std::fs::create_dir(root.join("src")).unwrap();
    std::fs::write(
        root.join("src/lib.rs"),
        b"pub fn uncanonicalized_root() {}\n",
    )
    .unwrap();

    let (text, bytes) =
        read_relative_regular(root, Path::new("src/lib.rs"), &permissive_safety(), 1024)
            .await
            .unwrap();

    assert_eq!(text, "pub fn uncanonicalized_root() {}\n");
    assert_eq!(bytes, b"pub fn uncanonicalized_root() {}\n");
}

#[cfg(unix)]
#[tokio::test]
async fn symlink_root_is_rejected() {
    use std::os::unix::fs::symlink;

    let temp = tempfile::tempdir().unwrap();
    let real_root = temp.path().join("real");
    std::fs::create_dir_all(real_root.join("src")).unwrap();
    std::fs::write(real_root.join("src/lib.rs"), b"pub fn secret() {}\n").unwrap();
    let link_root = temp.path().join("link");
    symlink(&real_root, &link_root).unwrap();

    let error = read_relative_regular(
        &link_root,
        Path::new("src/lib.rs"),
        &permissive_safety(),
        1024,
    )
    .await
    .unwrap_err();
    assert!(
        error
            .to_string()
            .contains("cannot pin artifact harness root"),
        "{error}"
    );
}

#[cfg(unix)]
#[tokio::test]
async fn leaf_symlink_outside_root_is_rejected() {
    use std::os::unix::fs::symlink;

    let root_temp = tempfile::Builder::new()
        .prefix(".primitive-safe-root-")
        .tempdir_in(".")
        .unwrap();
    let outside_temp = tempfile::Builder::new()
        .prefix(".primitive-safe-outside-")
        .tempdir_in(".")
        .unwrap();
    let root = std::fs::canonicalize(root_temp.path()).unwrap();
    let outside = outside_temp.path().join("secret.rs");
    std::fs::write(&outside, b"secret\n").unwrap();
    symlink(&outside, root.join("linked.rs")).unwrap();

    let error = read_relative_regular(&root, Path::new("linked.rs"), &permissive_safety(), 1024)
        .await
        .unwrap_err();
    assert!(error
        .to_string()
        .contains("artifact read refused linked.rs"));
}

#[cfg(unix)]
#[tokio::test]
async fn intermediate_directory_symlink_is_rejected() {
    use std::os::unix::fs::symlink;

    let root_temp = tempfile::Builder::new()
        .prefix(".primitive-safe-root-")
        .tempdir_in(".")
        .unwrap();
    let outside_temp = tempfile::Builder::new()
        .prefix(".primitive-safe-outside-")
        .tempdir_in(".")
        .unwrap();
    let root = std::fs::canonicalize(root_temp.path()).unwrap();
    std::fs::write(outside_temp.path().join("secret.rs"), b"secret\n").unwrap();
    symlink(outside_temp.path(), root.join("linked-dir")).unwrap();

    let error = read_relative_regular(
        &root,
        Path::new("linked-dir/secret.rs"),
        &permissive_safety(),
        1024,
    )
    .await
    .unwrap_err();
    assert!(error
        .to_string()
        .contains("artifact read refused directory component"));
}

#[cfg(unix)]
#[tokio::test]
async fn hard_link_to_an_outside_file_is_rejected() {
    let root_temp = tempfile::Builder::new()
        .prefix(".primitive-safe-root-")
        .tempdir_in(".")
        .unwrap();
    let outside_temp = tempfile::Builder::new()
        .prefix(".primitive-safe-outside-")
        .tempdir_in(".")
        .unwrap();
    let root = std::fs::canonicalize(root_temp.path()).unwrap();
    let outside = outside_temp.path().join("secret.rs");
    std::fs::write(&outside, b"outside secret\n").unwrap();
    std::fs::hard_link(&outside, root.join("looks-local.rs")).unwrap();

    let error = read_relative_regular(
        &root,
        Path::new("looks-local.rs"),
        &permissive_safety(),
        1024,
    )
    .await
    .unwrap_err();

    assert!(error.to_string().contains("multiple hard links"), "{error}");
}

#[cfg(unix)]
fn assert_same_filesystem_regular_read() {
    use std::os::unix::fs::MetadataExt;

    let temp = tempfile::Builder::new()
        .prefix(".primitive-safe-read-")
        .tempdir_in(".")
        .unwrap();
    let root = std::fs::canonicalize(temp.path()).unwrap();
    std::fs::create_dir(root.join("src")).unwrap();
    let file_path = root.join("src/lib.rs");
    std::fs::write(&file_path, b"pub fn safe() {}\n").unwrap();
    assert_eq!(
        std::fs::metadata(&root).unwrap().dev(),
        std::fs::metadata(&file_path).unwrap().dev(),
        "same-filesystem fixture left the pinned device"
    );

    let (text, bytes) = super::read_relative_regular_sync(
        &root,
        Path::new("src/lib.rs"),
        &permissive_safety(),
        1024,
    )
    .unwrap();
    assert_eq!(text, "pub fn safe() {}\n");
    assert_eq!(bytes, b"pub fn safe() {}\n");
}

// Proves the refusal predicate against a real second `st_dev` when the host has one.
// Hosts with a single device id fall through; the same-filesystem read already ran.
#[cfg(unix)]
fn foreign_device_descriptor_is_refused() {
    use std::os::unix::fs::MetadataExt;

    let temp = tempfile::Builder::new()
        .prefix(".primitive-safe-read-dev-")
        .tempdir_in(".")
        .unwrap();
    let root = std::fs::canonicalize(temp.path()).unwrap();
    let root_file = std::fs::File::open(&root).unwrap();
    let root_dev = root_file.metadata().unwrap().dev();
    let same_path = root.join("same.txt");
    std::fs::write(&same_path, b"same\n").unwrap();
    let same = std::fs::File::open(&same_path).unwrap();
    super::refuse_crossed_device(&same, root_dev, Path::new("same.txt"))
        .expect("same-device descriptor must be allowed");

    for candidate in [
        "/dev",
        "/dev/shm",
        "/proc",
        "/sys",
        "/run",
        "/System/Volumes/VM",
        "/System/Volumes/Preboot",
    ] {
        let Ok(foreign) = std::fs::File::open(candidate) else {
            continue;
        };
        let Ok(meta) = foreign.metadata() else {
            continue;
        };
        if meta.dev() == root_dev {
            continue;
        }
        let error = super::refuse_crossed_device(&foreign, root_dev, Path::new("foreign"))
            .expect_err("foreign device descriptor must be refused");
        assert!(
            error
                .to_string()
                .contains("the artifact crossed onto another device / mount"),
            "{error}"
        );
        return;
    }
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
fn command_succeeded(status: std::io::Result<std::process::ExitStatus>) -> bool {
    matches!(status, Ok(status) if status.success())
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
fn assert_prefix_would_allow(root: &Path, payload: &Path) {
    use std::os::unix::fs::MetadataExt;

    let meta = std::fs::metadata(payload).unwrap();
    assert!(meta.is_file(), "mounted fixture is not a regular file");
    assert_eq!(
        meta.nlink(),
        1,
        "mounted fixture tripped the hard-link check"
    );
    assert_ne!(
        meta.dev(),
        std::fs::metadata(root).unwrap().dev(),
        "mounted fixture stayed on the root device"
    );
    let real = std::fs::canonicalize(payload).unwrap();
    assert!(
        real.starts_with(root) && real != root,
        "mounted fixture {} is not under {}",
        real.display(),
        root.display()
    );
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
fn assert_cross_device_refused(root: &Path, relative: &Path) {
    let error = super::read_relative_regular_sync(root, relative, &permissive_safety(), 1024)
        .expect_err("cross-device artifact must be refused");
    assert!(
        error
            .to_string()
            .contains("the artifact crossed onto another device / mount"),
        "{error}"
    );
}

#[cfg(target_os = "macos")]
struct HdiutilDetach {
    mountpoint: std::path::PathBuf,
    image: std::path::PathBuf,
}

#[cfg(target_os = "macos")]
impl Drop for HdiutilDetach {
    fn drop(&mut self) {
        use std::process::{Command, Stdio};

        for target in [&self.mountpoint, &self.image] {
            let _ = Command::new("hdiutil")
                .args(["detach", "-quiet", "-force"])
                .arg(target)
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status();
        }
    }
}

#[cfg(target_os = "macos")]
fn hdiutil_attach(image: &Path, mountpoint: &Path) -> bool {
    use std::process::{Command, Stdio};

    let volname = format!("sr{}", std::process::id());
    if !command_succeeded(
        Command::new("hdiutil")
            .args([
                "create",
                "-quiet",
                "-size",
                "1m",
                "-fs",
                "HFS+",
                "-volname",
                volname.as_str(),
                "-ov",
            ])
            .arg(image)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status(),
    ) {
        return false;
    }
    command_succeeded(
        Command::new("hdiutil")
            .args([
                "attach",
                "-quiet",
                "-nobrowse",
                "-noautoopen",
                "-owners",
                "on",
                "-mountpoint",
            ])
            .arg(mountpoint)
            .arg(image)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status(),
    )
}

// Mounts a private image under `root/otherdev` when this process can do it
// without root. Failure to attach is "mount unavailable", not a test failure.
#[cfg(target_os = "macos")]
fn macos_hdiutil_cross_device_or_unavailable() {
    let root_temp = tempfile::Builder::new()
        .prefix(".primitive-safe-read-mount-")
        .tempdir_in(".")
        .unwrap();
    let image_temp = tempfile::Builder::new()
        .prefix(".primitive-safe-read-dmg-")
        .tempdir_in(".")
        .unwrap();
    let root = std::fs::canonicalize(root_temp.path()).unwrap();
    let mountpoint = root.join("otherdev");
    std::fs::create_dir(&mountpoint).unwrap();
    let image = image_temp.path().join("probe.dmg");
    if !hdiutil_attach(&image, &mountpoint) {
        return;
    }
    let _detach = HdiutilDetach {
        mountpoint: mountpoint.clone(),
        image,
    };
    let payload = mountpoint.join("payload.txt");
    std::fs::write(&payload, b"other-device\n").unwrap();
    // Still under the pinned prefix with nlink == 1; only st_dev differs.
    assert_prefix_would_allow(&root, &payload);
    assert_cross_device_refused(&root, Path::new("otherdev/payload.txt"));
}

#[cfg(target_os = "linux")]
struct UmountGuard(std::path::PathBuf);

#[cfg(target_os = "linux")]
impl Drop for UmountGuard {
    fn drop(&mut self) {
        use std::process::{Command, Stdio};

        let status = Command::new("umount")
            .arg(&self.0)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
        if !command_succeeded(status) {
            let _ = Command::new("umount")
                .args(["-l"])
                .arg(&self.0)
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status();
        }
    }
}

#[cfg(target_os = "linux")]
fn mount_tmpfs(mountpoint: &Path) -> bool {
    use std::process::{Command, Stdio};

    command_succeeded(
        Command::new("mount")
            .args(["-t", "tmpfs", "-o", "size=1m,mode=0700", "tmpfs"])
            .arg(mountpoint)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status(),
    )
}

#[cfg(target_os = "linux")]
fn write_mounted_payload(root: &Path, mountpoint: &Path) {
    let payload = mountpoint.join("payload.txt");
    std::fs::write(&payload, b"other-device\n").unwrap();
    assert_prefix_would_allow(root, &payload);
    assert_cross_device_refused(root, Path::new("otherdev/payload.txt"));
}

#[cfg(target_os = "linux")]
fn try_privileged_tmpfs_refusal() -> bool {
    let root_temp = tempfile::Builder::new()
        .prefix(".primitive-safe-read-mount-")
        .tempdir_in(".")
        .unwrap();
    let root = std::fs::canonicalize(root_temp.path()).unwrap();
    let mountpoint = root.join("otherdev");
    std::fs::create_dir(&mountpoint).unwrap();
    if !mount_tmpfs(&mountpoint) {
        return false;
    }
    let _umount = UmountGuard(mountpoint.clone());
    write_mounted_payload(&root, &mountpoint);
    true
}

#[cfg(target_os = "linux")]
fn write_probe_result(status: &str) {
    let Ok(path) = std::env::var("SELFWARE_SAFE_READ_MOUNT_RESULT") else {
        return;
    };
    let _ = std::fs::write(path, status);
}

#[cfg(target_os = "linux")]
fn linux_mount_namespace_child() {
    let root_temp = tempfile::Builder::new()
        .prefix(".primitive-safe-read-mount-")
        .tempdir_in(".")
        .unwrap();
    let root = std::fs::canonicalize(root_temp.path()).unwrap();
    let mountpoint = root.join("otherdev");
    std::fs::create_dir(&mountpoint).unwrap();
    if !mount_tmpfs(&mountpoint) {
        write_probe_result("UNAVAILABLE");
        return;
    }
    let _umount = UmountGuard(mountpoint.clone());
    write_mounted_payload(&root, &mountpoint);
    write_probe_result("REFUSED");
}

#[cfg(target_os = "linux")]
fn assert_probe_outcome(output: &std::process::Output, recorded: &str) {
    let recorded = recorded.trim();
    if (recorded == "REFUSED" && output.status.success()) || recorded == "UNAVAILABLE" {
        return;
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    if stdout.contains("running 0 tests") {
        panic!("cross-device probe did not run the refusal test\n{stdout}\n{stderr}");
    }
    let started = stdout.contains("running 1 test")
        || stdout.contains("panicked at")
        || stderr.contains("panicked at");
    if started || recorded == "REFUSED" {
        let detail = format!(
            "status {:?}\nstdout:\n{stdout}\nstderr:\n{stderr}\nresult: {recorded}",
            output.status.code()
        );
        panic!("cross-device mount probe failed\n{detail}");
    }
}

#[cfg(target_os = "linux")]
fn cross_device_test_name() -> String {
    // libtest filters omit the crate name that `module_path!` includes.
    let module_path = module_path!();
    let module_path = module_path
        .split_once("::")
        .map(|(_, rest)| rest)
        .unwrap_or(module_path);
    format!("{module_path}::same_filesystem_read_succeeds_and_cross_device_mount_is_refused")
}

#[cfg(target_os = "linux")]
fn linux_cross_device_or_unavailable() {
    use std::process::{Command, Stdio};

    if try_privileged_tmpfs_refusal() {
        return;
    }
    let result_dir = tempfile::Builder::new()
        .prefix(".primitive-safe-read-probe-")
        .tempdir_in(".")
        .unwrap();
    let result_path = std::fs::canonicalize(result_dir.path())
        .unwrap()
        .join("result");
    let Ok(exe) = std::env::current_exe() else {
        return;
    };
    let test_name = cross_device_test_name();
    let output = Command::new("unshare")
        .args([
            "--user",
            "--map-root-user",
            "--mount",
            "--propagation",
            "private",
            "--",
        ])
        .arg(exe)
        .arg("--exact")
        .arg(test_name)
        .arg("--test-threads")
        .arg("1")
        .env("SELFWARE_SAFE_READ_MOUNT_CHILD", "1")
        .env("SELFWARE_SAFE_READ_MOUNT_RESULT", &result_path)
        .env("RUST_TEST_THREADS", "1")
        .stdin(Stdio::null())
        .output();
    let Ok(output) = output else {
        return;
    };
    let recorded = std::fs::read_to_string(&result_path).unwrap_or_default();
    assert_probe_outcome(&output, &recorded);
}

#[cfg(unix)]
#[test]
fn same_filesystem_read_succeeds_and_cross_device_mount_is_refused() {
    #[cfg(target_os = "linux")]
    if std::env::var_os("SELFWARE_SAFE_READ_MOUNT_CHILD").is_some() {
        linux_mount_namespace_child();
        return;
    }

    assert_same_filesystem_regular_read();
    foreign_device_descriptor_is_refused();

    #[cfg(target_os = "macos")]
    macos_hdiutil_cross_device_or_unavailable();
    #[cfg(target_os = "linux")]
    linux_cross_device_or_unavailable();
}
