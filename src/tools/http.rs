//! HTTP request tool for web/API interactions

use super::Tool;
use anyhow::{Context, Result};
use async_trait::async_trait;
use reqwest::Client;
use serde::Deserialize;
use serde_json::Value;
use std::collections::HashMap;
use std::net::IpAddr;
use std::time::Duration;

pub struct HttpRequest;

#[async_trait]
impl Tool for HttpRequest {
    fn name(&self) -> &str {
        "http_request"
    }

    fn description(&self) -> &str {
        "Make HTTP requests to APIs or web endpoints. Supports GET, POST, PUT, DELETE methods. \
         Use for fetching documentation, calling APIs, or testing endpoints."
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

        let args: Args = serde_json::from_value(args)?;

        // Validate URL
        let url = reqwest::Url::parse(&args.url).context("Invalid URL")?;

        // Block potentially dangerous URLs
        if url.scheme() != "http" && url.scheme() != "https" {
            anyhow::bail!("Only HTTP and HTTPS URLs are allowed");
        }

        // Block requests to private/internal network addresses (SSRF protection).
        // Set SELFWARE_ALLOW_PRIVATE_NETWORK=1 to override for local development.
        if let Some(host) = url.host_str() {
            if is_private_network_host(host) {
                let allow_private =
                    std::env::var("SELFWARE_ALLOW_PRIVATE_NETWORK").unwrap_or_default() == "1";
                if !allow_private {
                    anyhow::bail!(
                        "Blocked request to private/internal network address: {}. \
                         Set SELFWARE_ALLOW_PRIVATE_NETWORK=1 to allow.",
                        host
                    );
                }
                tracing::warn!(
                    "Allowing request to private network (SELFWARE_ALLOW_PRIVATE_NETWORK=1): {}",
                    host
                );
            }
        }

        // Build client
        let client = Client::builder()
            .timeout(Duration::from_secs(args.timeout_secs))
            .redirect(if args.follow_redirects {
                reqwest::redirect::Policy::limited(10)
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
            format!(
                "{}...[truncated, {} bytes total]",
                &body[..50000],
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

/// Check whether a hostname or IP belongs to a private/internal network range.
fn is_private_network_host(host: &str) -> bool {
    if host == "localhost" || host.ends_with(".localhost") {
        return true;
    }
    let bare_host = host.trim_start_matches('[').trim_end_matches(']');
    if let Ok(ip) = bare_host.parse::<IpAddr>() {
        return match ip {
            IpAddr::V4(v4) => {
                v4.is_loopback() || v4.is_private() || v4.is_link_local() || v4.is_unspecified()
            }
            IpAddr::V6(v6) => {
                v6.is_loopback()
                    || v6.is_unspecified()
                    || (v6.segments()[0] & 0xffc0) == 0xfe80 // fe80::/10
                    || (v6.segments()[0] & 0xfe00) == 0xfc00 // fc00::/7
            }
        };
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_http_request_name() {
        let tool = HttpRequest;
        assert_eq!(tool.name(), "http_request");
    }

    #[test]
    fn test_http_request_description() {
        let tool = HttpRequest;
        assert!(tool.description().contains("HTTP"));
        assert!(tool.description().contains("API"));
    }

    #[test]
    fn test_http_request_schema() {
        let tool = HttpRequest;
        let schema = tool.schema();
        assert_eq!(schema["type"], "object");
        assert!(schema["properties"]["url"].is_object());
        assert!(schema["properties"]["method"].is_object());
        assert!(schema["properties"]["headers"].is_object());
    }

    #[test]
    fn test_http_request_schema_methods() {
        let tool = HttpRequest;
        let schema = tool.schema();
        let methods = schema["properties"]["method"]["enum"].as_array().unwrap();
        assert!(methods.contains(&serde_json::json!("GET")));
        assert!(methods.contains(&serde_json::json!("POST")));
        assert!(methods.contains(&serde_json::json!("PUT")));
        assert!(methods.contains(&serde_json::json!("DELETE")));
    }

    #[tokio::test]
    async fn test_http_request_invalid_url() {
        let tool = HttpRequest;
        let result = tool
            .execute(serde_json::json!({
                "url": "not-a-valid-url"
            }))
            .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_http_request_invalid_scheme() {
        let tool = HttpRequest;
        let result = tool
            .execute(serde_json::json!({
                "url": "ftp://example.com/file"
            }))
            .await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("HTTP"));
    }

    #[tokio::test]
    async fn test_http_request_invalid_method() {
        let tool = HttpRequest;
        let result = tool
            .execute(serde_json::json!({
                "url": "https://example.com",
                "method": "INVALID"
            }))
            .await;
        assert!(result.is_err());
    }

    #[test]
    fn test_http_request_schema_required() {
        let tool = HttpRequest;
        let schema = tool.schema();
        let required = schema["required"].as_array().unwrap();
        assert!(required.contains(&serde_json::json!("url")));
    }

    #[test]
    fn test_http_request_schema_has_timeout() {
        let tool = HttpRequest;
        let schema = tool.schema();
        assert!(schema["properties"]["timeout_secs"].is_object());
    }

    #[test]
    fn test_http_request_schema_has_body() {
        let tool = HttpRequest;
        let schema = tool.schema();
        assert!(schema["properties"]["body"].is_object());
    }

    #[tokio::test]
    async fn test_http_request_missing_url() {
        let tool = HttpRequest;
        let result = tool.execute(serde_json::json!({})).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_http_request_file_scheme() {
        let tool = HttpRequest;
        let result = tool
            .execute(serde_json::json!({
                "url": "file:///etc/passwd"
            }))
            .await;
        assert!(result.is_err());
    }
}
