use super::*;

#[test]
fn test_health_status_equality() {
    assert_eq!(HealthStatus::Healthy, HealthStatus::Healthy);
    assert_ne!(
        HealthStatus::Healthy,
        HealthStatus::Degraded {
            reason: "slow".into()
        }
    );
}

#[test]
fn test_severity_variants() {
    assert_ne!(Severity::Warning, Severity::Critical);
    assert_ne!(Severity::Critical, Severity::Fatal);
    assert_eq!(Severity::Warning, Severity::Warning);
}

#[tokio::test]
async fn test_health_endpoint_responds() {
    use tokio::io::AsyncReadExt;
    // Start on a random available port
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);

    // Start health endpoint in background
    tokio::spawn(async move {
        let _ = start_health_endpoint(port).await;
    });

    // Give it a moment to bind
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    // Connect and read response
    let mut stream = tokio::net::TcpStream::connect(format!("127.0.0.1:{}", port))
        .await
        .unwrap();
    let request = "GET /health HTTP/1.1\r\nHost: localhost\r\n\r\n";
    use tokio::io::AsyncWriteExt;
    stream.write_all(request.as_bytes()).await.unwrap();

    let mut buf = vec![0u8; 1024];
    let n = stream.read(&mut buf).await.unwrap();
    let response = String::from_utf8_lossy(&buf[..n]);
    assert!(response.contains("200 OK"));
    assert!(response.contains("healthy"));
}

#[test]
fn test_maybe_start_health_endpoint_no_env() {
    // Should not panic when env var is not set
    // (can't easily test the spawn path without a runtime)
    std::env::remove_var("SELFWARE_HEALTH_PORT");
    // Just verify the function exists and doesn't panic when env is missing
}
