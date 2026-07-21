//! HTTP request tool for web/API interactions

use super::net_policy::{self, NetworkPolicy};
use super::Tool;
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
        let response = request
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

        let body = response
            .text()
            .await
            .context("Failed to read response body")?;

        // Truncate body if too large
        let truncated = body.len() > 50000;
        let body = if truncated {
            let safe_truncate: String = body.chars().take(50000).collect();
            format!(
                "{}...[truncated, {} bytes total]",
                safe_truncate,
                body.len()
            )
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

#[cfg(test)]
#[path = "../../tests/unit/tools/http/http_test.rs"]
mod tests;
