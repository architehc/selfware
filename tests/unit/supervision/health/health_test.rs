use super::*;

#[test]
fn never_pinged_is_healthy() {
    set_last_heartbeat_ms_for_test(0);
    assert!(
        heartbeat_healthy(),
        "no heartbeat wired -> healthy (bare-bones)"
    );
}

#[test]
fn fresh_heartbeat_is_healthy() {
    record_heartbeat();
    assert!(heartbeat_healthy(), "a just-recorded heartbeat is healthy");
}

#[test]
fn stale_heartbeat_is_unhealthy() {
    // A heartbeat far older than the default 900s threshold is stale.
    let old = now_ms().saturating_sub(2_000_000); // ~2000s ago
    set_last_heartbeat_ms_for_test(old.max(1));
    assert!(
        !heartbeat_healthy(),
        "a heartbeat older than the staleness threshold must be unhealthy"
    );
    // Restore a fresh heartbeat so we don't leave the global stale for other
    // tests that might observe it.
    record_heartbeat();
}

#[tokio::test]
async fn health_endpoint_serves_http_get_without_rst() {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::{TcpListener, TcpStream};

    record_heartbeat();
    set_global_healthy(true);

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    let server = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        handle_health_connection(stream).await;
    });

    let mut client = TcpStream::connect(addr).await.unwrap();
    client
        .write_all(b"GET /health HTTP/1.1\r\nHost: localhost\r\nUser-Agent: test\r\n\r\n")
        .await
        .unwrap();

    let mut response = String::new();
    client.read_to_string(&mut response).await.unwrap();
    server.await.unwrap();

    assert!(
        response.starts_with("HTTP/1.1 200 OK\r\n"),
        "got: {response}"
    );
    assert!(
        response.contains("Content-Length: 8\r\n"),
        "got: {response}"
    );
    assert!(response.ends_with("\r\n\r\nhealthy\n"), "got: {response}");
}
