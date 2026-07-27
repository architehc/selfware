//! Embedded-web fallback tests for the evolve server (release-binary asset
//! serving). Included from `src/evolve/server.rs` via `#[path]`, so `super::*`
//! reaches the private `with_web_fallback` / `embedded_asset` items.
//!
//! Contract: with no `src/evolve/web` on disk (a released binary run outside a
//! source checkout), GET / and /app.js return 200 with the embedded content;
//! vendor/ 404s (editor mode requires a source checkout). With the dir on
//! disk, the dev override serves from disk so UI iteration needs no rebuild.

use super::*;
use axum::body::Body;
use axum::http::Request;
use tower::ServiceExt;

/// Router with the web fallback pointed at a path that does not exist — the
/// release-binary situation (CWD has no `src/evolve/web`).
fn embedded_router() -> Router {
    with_web_fallback(
        Router::new(),
        Path::new("/definitely/not/a/source/checkout/src/evolve/web"),
    )
}

async fn get(router: Router, uri: &str) -> (StatusCode, Option<String>, String) {
    let response = router
        .oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap())
        .await
        .unwrap();
    let status = response.status();
    let content_type = response
        .headers()
        .get(axum::http::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .map(str::to_string);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    (
        status,
        content_type,
        String::from_utf8(body.to_vec()).unwrap(),
    )
}

#[tokio::test]
async fn index_is_served_from_embedded_assets_without_web_dir() {
    let (status, content_type, body) = get(embedded_router(), "/").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(content_type.as_deref(), Some("text/html; charset=utf-8"));
    assert_eq!(body, include_str!("../../../src/evolve/web/index.html"));
}

#[tokio::test]
async fn app_js_is_served_from_embedded_assets_without_web_dir() {
    let (status, content_type, body) = get(embedded_router(), "/app.js").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        content_type.as_deref(),
        Some("text/javascript; charset=utf-8")
    );
    assert_eq!(body, include_str!("../../../src/evolve/web/app.js"));
}

#[tokio::test]
async fn style_and_editor_are_served_from_embedded_assets() {
    let (status, content_type, body) = get(embedded_router(), "/style.css").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(content_type.as_deref(), Some("text/css; charset=utf-8"));
    assert_eq!(body, include_str!("../../../src/evolve/web/style.css"));

    let (status, _, body) = get(embedded_router(), "/editor.html").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, include_str!("../../../src/evolve/web/editor.html"));
}

#[tokio::test]
async fn vendor_and_unknown_paths_404_in_embedded_mode() {
    // vendor/ (d3/lucide/monaco) is only needed by the editor, which requires
    // a source checkout — embedded mode deliberately does not ship it.
    let (status, _, body) = get(embedded_router(), "/vendor/d3/d3.min.js").await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert!(body.contains("source checkout"), "{body}");

    let (status, _, _) = get(embedded_router(), "/no-such-file.png").await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn existing_web_dir_is_served_from_disk_dev_override() {
    // A source checkout (web dir on disk) wins over the embedded assets, so
    // UI iteration doesn't need rebuilds.
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("index.html"), "disk index\n").unwrap();
    std::fs::write(dir.path().join("extra.js"), "disk only\n").unwrap();
    let router = with_web_fallback(Router::new(), dir.path());

    let (status, _, body) = get(router, "/").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, "disk index\n");

    // Files that exist only on disk (not in the embedded map) are served too.
    let router = with_web_fallback(Router::new(), dir.path());
    let (status, _, body) = get(router, "/extra.js").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, "disk only\n");
}
