//! HTTP request tool for web/API interactions

use super::net_policy::{self, NetworkPolicy};
use super::Tool;
use crate::safety::checker::validation::contains_outbound_credential_shape;
use crate::safety::PinnedDnsResolver;
use anyhow::{Context, Result};
use async_trait::async_trait;
use reqwest::Client;
use serde::Deserialize;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

pub struct HttpRequest;

/// Thin alias so that call-sites inside this file don't change shape.
type HttpTargetPolicy = NetworkPolicy;

#[async_trait]
impl Tool for HttpRequest {
    fn name(&self) -> &str {
        "http_request"
    }

    fn description(&self) -> &str {
        "Make HTTP requests to APIs or web endpoints. Supports GET, POST, PUT, DELETE methods. \
         Use for fetching documentation, calling APIs, or testing endpoints. \
         Localhost/loopback URLs are allowed by default; set SELFWARE_ALLOW_PRIVATE_NETWORK=1 for private LAN hosts."
    }

    fn schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "url": {
                    "type": "string",
                    "description": "The URL to request"
                },
                "method": {
                    "type": "string",
                    "enum": ["GET", "POST", "PUT", "DELETE", "PATCH", "HEAD"],
                    "default": "GET",
                    "description": "HTTP method"
                },
                "headers": {
                    "type": "object",
                    "additionalProperties": {"type": "string"},
                    "description": "Request headers"
                },
                "body": {
                    "type": "string",
                    "description": "Request body (for POST/PUT/PATCH)"
                },
                "timeout_secs": {
                    "type": "integer",
                    "default": 30,
                    "description": "Request timeout in seconds"
                },
                "follow_redirects": {
                    "type": "boolean",
                    "default": true,
                    "description": "Whether to follow redirects"
                }
            },
            "required": ["url"]
        })
    }

    async fn execute(&self, args: Value) -> Result<Value> {
        #[derive(Deserialize)]
        struct Args {
            url: String,
            #[serde(default = "default_method")]
            method: String,
            #[serde(default)]
            headers: HashMap<String, String>,
            body: Option<String>,
            #[serde(default = "default_timeout")]
            timeout_secs: u64,
            #[serde(default = "default_true")]
            follow_redirects: bool,
        }

        fn default_method() -> String {
            "GET".to_string()
        }
        fn default_timeout() -> u64 {
            30
        }
        fn default_true() -> bool {
            true
        }

        let mut args: Args = serde_json::from_value(args)?;

        // Cap timeout to prevent indefinite hangs (5 minutes max for HTTP)
        const MAX_TIMEOUT_SECS: u64 = 300;
        args.timeout_secs = args.timeout_secs.min(MAX_TIMEOUT_SECS);

        // Validate URL
        let url = reqwest::Url::parse(&args.url).context("Invalid URL")?;

        let allow_private =
            std::env::var("SELFWARE_ALLOW_PRIVATE_NETWORK").unwrap_or_default() == "1";
        let policy = validate_http_request_target(&url, allow_private)?;

        // Outbound-content secret policy — enforced HERE at request
        // construction, not just by the safety checker on tool calls, so a
        // direct invocation can't bypass it by payload location. A
        // credential-shaped value (ghp_…, sk_live_…, …) must not leave the
        // workspace in the URL, the body, or any header value. The checker
        // alongside this tool uses the same shape oracle
        // (`contains_outbound_credential_shape`). Operator-configured
        // authentication is treated by shape, not by header name: a value
        // the operator deliberately provisions is refused only when it
        // matches a known leaked-credential prefix (whitelisting
        // `Authorization` by name would re-open the URL/body/header bypass
        // this closes); bearer tokens with no known shape (JWTs, random
        // session keys) pass.
        reject_outbound_credential_shapes(&args.url, &args.headers, args.body.as_deref())?;

        // SSRF protection: use PinnedDnsResolver to resolve DNS once and reject
        // private/internal IPs at resolution time. This prevents DNS rebinding
        // attacks where a hostname resolves to a public IP during validation but
        // to a private IP (e.g., 169.254.169.254) during the actual connection.

        let builder = Client::builder()
            .timeout(Duration::from_secs(args.timeout_secs))
            .dns_resolver(Arc::new(PinnedDnsResolver::new(
                policy.allow_private || policy.allow_localhost,
            )));

        if let Some(host) = url.host_str() {
            if net_policy::is_private_network_host(host)
                && policy.allow_private
                && !policy.allow_localhost
            {
                tracing::warn!(
                    "Allowing request to private network (SELFWARE_ALLOW_PRIVATE_NETWORK=1): {}",
                    host
                );
            }
        }

        let client = builder
            .redirect(if args.follow_redirects {
                reqwest::redirect::Policy::custom(move |attempt| {
                    if attempt.previous().len() > 10 {
                        return attempt.error("Too many redirects");
                    }
                    // Check redirect targets for known-private hostnames (e.g. "localhost").
                    // DNS-level protection for redirects is handled by PinnedDnsResolver,
                    // which will reject any resolution to a private IP.
                    if let Some(host) = attempt.url().host_str().map(|h| h.to_owned()) {
                        if !policy.allow_private
                            && !net_policy::is_trusted_local_network_host(&host)
                            && net_policy::is_private_network_host(&host)
                        {
                            return attempt
                                .error("Blocked redirect to private/internal network address");
                        }
                    }
                    attempt.follow()
                })
            } else {
                reqwest::redirect::Policy::none()
            })
            .build()
            .context("Failed to build HTTP client")?;

        // Build request
        let mut request = match args.method.to_uppercase().as_str() {
            "GET" => client.get(&args.url),
            "POST" => client.post(&args.url),
            "PUT" => client.put(&args.url),
            "DELETE" => client.delete(&args.url),
            "PATCH" => client.patch(&args.url),
            "HEAD" => client.head(&args.url),
            _ => anyhow::bail!("Unsupported HTTP method: {}", args.method),
        };

        // Add headers
        for (key, value) in &args.headers {
            request = request.header(key, value);
        }

        // Add body if present
        if let Some(body) = args.body {
            request = request.body(body);
        }

        // Execute request
        let start = std::time::Instant::now();
        let mut response = request
            .send()
            .await
            .context("Failed to send HTTP request")?;

        let duration_ms = start.elapsed().as_millis() as u64;
        let status = response.status().as_u16();
        let status_text = response.status().canonical_reason().unwrap_or("Unknown");

        // Collect response headers
        let response_headers: HashMap<String, String> = response
            .headers()
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_str().unwrap_or("").to_string()))
            .collect();

        // Get response body
        let content_type = response_headers
            .get("content-type")
            .cloned()
            .unwrap_or_default();

        // Stream the body into a bounded buffer. Display truncation is 50K
        // chars, but the ALLOCATION must be capped too (review finding: a
        // large in-timeout response could exhaust memory before the truncate
        // ran). Read chunks until the cap + overflow marker, then stop; the
        // connection drops with the response.
        const BODY_CAP: usize = 51_200; // 50 KiB
        let mut buf: Vec<u8> = Vec::new();
        let mut hit_cap = false;
        while let Some(chunk) = response
            .chunk()
            .await
            .context("Failed to read response body")?
        {
            if buf.len() + chunk.len() > BODY_CAP {
                buf.extend_from_slice(&chunk[..BODY_CAP - buf.len()]);
                hit_cap = true;
                break;
            }
            buf.extend_from_slice(&chunk);
        }
        let body = String::from_utf8_lossy(&buf).into_owned();

        // Truncate body if too large
        let truncated = hit_cap || body.len() > 50000;
        let body = if truncated {
            let safe_truncate: String = body.chars().take(50000).collect();
            if hit_cap {
                format!("{safe_truncate}...[truncated at 50 KiB cap]")
            } else {
                format!(
                    "{}...[truncated, {} bytes total]",
                    safe_truncate,
                    body.len()
                )
            }
        } else {
            body
        };

        // Try to parse as JSON if content type suggests it
        let body_json: Option<Value> = if content_type.contains("application/json") {
            serde_json::from_str(&body).ok()
        } else {
            None
        };

        Ok(serde_json::json!({
            "status": status,
            "status_text": status_text,
            "headers": response_headers,
            "body": body,
            "body_json": body_json,
            "duration_ms": duration_ms,
            "truncated": truncated
        }))
    }
}

/// Validate the target of an HTTP request, delegating to the shared
/// `net_policy` module.
fn validate_http_request_target(
    url: &reqwest::Url,
    allow_private: bool,
) -> Result<HttpTargetPolicy> {
    // `url::Url` and `reqwest::Url` are the same type (reqwest re-exports url).
    net_policy::validate_url_target(url, allow_private)
}

/// Refuse an outgoing request when any payload location — URL, header
/// value, or body — carries a known credential shape. This is the
/// http_request tool's half of the outbound-content policy; the safety
/// checker applies the identical oracle to tool calls, and this covers
/// direct invocations that skip the gate.
///
/// Authentication that the operator deliberately configured is honored by
/// SHAPE: values with no known leaked-credential prefix (JWT bearer tokens,
/// random keys, `Basic base64` for an internal service) pass; values that
/// match a known credential prefix are refused wherever they appear,
/// including an `Authorization` header — the model's tool-call payload is
/// attacker-controlled, and whitelisting that header name would restore the
/// URL-vs-body/header bypass this policy closes.
fn reject_outbound_credential_shapes(
    url: &str,
    headers: &HashMap<String, String>,
    body: Option<&str>,
) -> Result<()> {
    if contains_outbound_credential_shape(url) {
        anyhow::bail!("Refusing outbound request: credential-shaped value in URL");
    }
    if let Some(body) = body {
        if contains_outbound_credential_shape(body) {
            anyhow::bail!("Refusing outbound request: credential-shaped value in request body");
        }
    }
    for (name, value) in headers {
        if contains_outbound_credential_shape(value) {
            anyhow::bail!("Refusing outbound request: credential-shaped value in header {name}");
        }
    }
    Ok(())
}

#[cfg(test)]
#[path = "../../tests/unit/tools/http/http_test.rs"]
mod tests;
