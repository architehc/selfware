//! FunctionGemma Endpoint Scout & Automatic Failover (`selfware`).
//!
//! When the configured primary LLM endpoint is down (`FailureKind::LlmUnreachable`,
//! connection refused, DNS failure, or an offline ngrok/Cloudflare tunnel),
//! Selfware can load its local **FunctionGemma 270M IT** (`functiongemma-270m-it-UD-Q6_K_XL.gguf`
//! on `llama-server` port `8787`), probe candidate local/LAN/cloud endpoints,
//! and use FunctionGemma to select a verified live endpoint (or serve as the
//! offline fallback endpoint itself when all external servers are down).
//!
//! Safety invariants:
//! - FunctionGemma NEVER invents an unverified URL: it only selects an index from
//!   the pre-probed `Vec<ScoutedEndpoint>` list that passed a live `/models` health check.
//! - Hosted API keys (`config.api_key`) are NEVER forwarded to unrelated third-party
//!   fallback endpoints; a candidate only receives a key if it comes from its own
//!   explicit `[models.<name>]` profile or `SELFWARE_LOCAL_API_KEY`.

use std::time::{Duration, Instant};

use super::model::{
    boot_dir, ensure_background_boot_server_for_model_with_context, find_bundled_model,
    find_llama_server, is_boot_server_healthy, model_path_in, verify_model_file,
    FUNCTIONGEMMA_CTX_32K, SERVER_ALIAS, SERVER_PORT,
};

/// A candidate endpoint that passed a live `/v1/models` reachability probe.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScoutedEndpoint {
    /// Human-readable provider label (e.g. `"LAN vLLM/SGLang"`, `"Ollama"`, `"FunctionGemma 270M"`).
    pub provider: String,
    /// OpenAI-compatible base URL (ending in `/v1`).
    pub endpoint: String,
    /// Verified model ID served by this endpoint.
    pub model: String,
    /// Context window reported by `/v1/models` or inferred from model profile.
    pub context_length: usize,
    /// Round-trip latency of the `/v1/models` probe in milliseconds.
    pub latency_ms: u64,
    /// Optional `[models.<name>]` profile key from `selfware.toml` that matched this endpoint.
    pub profile_name: Option<String>,
}

/// Result of running the FunctionGemma Endpoint Scout when an endpoint is down.
#[derive(Debug, Clone)]
pub struct EndpointScoutReport {
    /// The endpoint that failed and triggered scouting.
    pub failed_endpoint: String,
    /// All candidate endpoints that passed the live `/v1/models` probe.
    pub reachable: Vec<ScoutedEndpoint>,
    /// Index into `reachable` chosen by FunctionGemma (or deterministic ranker).
    pub selected_index: Option<usize>,
    /// Whether FunctionGemma 270M IT was loaded and consulted for the selection.
    pub functiongemma_used: bool,
    /// Raw reasoning / tool call returned by FunctionGemma (for auditability in logs).
    pub functiongemma_rationale: Option<String>,
}

impl EndpointScoutReport {
    /// Return the selected [`ScoutedEndpoint`], if any candidate was reachable.
    pub fn selected(&self) -> Option<&ScoutedEndpoint> {
        self.selected_index.and_then(|idx| self.reachable.get(idx))
    }

    /// Concise human-readable summary for TUI chat/log banners.
    pub fn summary_line(&self) -> String {
        match self.selected() {
            Some(chosen) => {
                let via = if self.functiongemma_used {
                    "via FunctionGemma 270M scout"
                } else {
                    "via deterministic endpoint probe"
                };
                format!(
                    "🦊 Endpoint failover ({via}): switched from `{}` → `{}` ({} · model `{}` · ctx {} · {}ms)",
                    crate::config::redact_endpoint(&self.failed_endpoint),
                    crate::config::redact_endpoint(&chosen.endpoint),
                    chosen.provider,
                    chosen.model,
                    chosen.context_length,
                    chosen.latency_ms
                )
            }
            None => format!(
                "⚠️ Endpoint `{}` is unreachable and no live fallback endpoints responded.",
                crate::config::redact_endpoint(&self.failed_endpoint)
            ),
        }
    }
}

/// Returns `true` when `error` indicates the LLM endpoint is down, unreachable,
/// or behind an offline tunnel/gateway (e.g. `ERR_NGROK_3200`, `Connection refused`,
/// DNS lookup failure, `502 Bad Gateway`, `503 Service Unavailable`).
pub fn is_endpoint_down_error(error: &str) -> bool {
    #[cfg(feature = "resilience")]
    if crate::self_healing::recovery_tree::classify(error)
        == crate::self_healing::FailureKind::LlmUnreachable
    {
        return true;
    }
    let lower = error.to_ascii_lowercase();
    lower.contains("err_ngrok_3200")
        || lower.contains("err_ngrok_8012")
        || (lower.contains("tunnel") && lower.contains("offline"))
        || lower.contains("502 bad gateway")
        || lower.contains("503 service unavailable")
        || lower.contains("connection refused")
        || lower.contains("error trying to connect")
        || lower.contains("failed to lookup address")
        || lower.contains("dns error")
        || lower.contains("network is unreachable")
        || lower.contains("no route to host")
}

/// Build the ordered list of candidate endpoints to probe when `config.endpoint` is down.
/// Each tuple is `(provider_label, endpoint_url, preferred_model, context_hint, profile_name)`.
pub fn candidate_endpoints_for_config(
    config: &crate::config::Config,
) -> Vec<(
    String,
    String,
    Option<String>,
    Option<usize>,
    Option<String>,
)> {
    let current_norm = config.endpoint.trim().trim_end_matches('/').to_string();
    let mut out: Vec<(
        String,
        String,
        Option<String>,
        Option<usize>,
        Option<String>,
    )> = Vec::new();

    let mut push_unique = |provider: String,
                           endpoint: String,
                           model: Option<String>,
                           ctx: Option<usize>,
                           profile_name: Option<String>| {
        let norm = endpoint.trim().trim_end_matches('/').to_string();
        if norm.is_empty() || norm.eq_ignore_ascii_case(&current_norm) {
            return;
        }
        if out.iter().any(|(_, existing, _, _, _)| {
            existing.trim_end_matches('/').eq_ignore_ascii_case(&norm)
        }) {
            return;
        }
        out.push((provider, endpoint, model, ctx, profile_name));
    };

    // 1. Explicit `[models.<name>]` profiles from `config.models`
    let mut profile_entries: Vec<_> = config.models.iter().collect();
    profile_entries.sort_by_key(|(name, _)| *name);
    for (name, profile) in profile_entries {
        if *name == "default" {
            continue;
        }
        push_unique(
            format!("Profile [{name}]"),
            profile.endpoint.clone(),
            (!profile.model.trim().is_empty()).then(|| profile.model.clone()),
            Some(profile.context_length),
            Some(name.clone()),
        );
    }

    // 2. Explicit environment fallback overrides
    for env_key in ["SELFWARE_FALLBACK_ENDPOINT", "SELFWARE_LOCAL_ENDPOINT"] {
        if let Ok(val) = std::env::var(env_key) {
            let trimmed = val.trim();
            if !trimmed.is_empty() {
                let model_override = std::env::var("SELFWARE_LOCAL_MODEL")
                    .ok()
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty());
                push_unique(
                    format!("Env ({env_key})"),
                    trimmed.to_string(),
                    model_override,
                    None,
                    None,
                );
            }
        }
    }

    // 3. Known paired test-server sibling (`https://ivo.ngrok.dev/v1` <-> `http://192.168.1.224:8000/v1`)
    if current_norm.contains("ivo.ngrok.dev") {
        push_unique(
            "LAN Test Server (192.168.1.224)".to_string(),
            "http://192.168.1.224:8000/v1".to_string(),
            Some(config.model.clone()),
            Some(config.context_length),
            None,
        );
    } else if current_norm.contains("192.168.1.224") {
        push_unique(
            "Ngrok Test Tunnel (ivo.ngrok.dev)".to_string(),
            "https://ivo.ngrok.dev/v1".to_string(),
            Some(config.model.clone()),
            Some(config.context_length),
            None,
        );
    }

    // 4. Standard local LLM servers
    push_unique(
        "Local vLLM / SGLang (:8000)".to_string(),
        "http://localhost:8000/v1".to_string(),
        None,
        None,
        None,
    );
    push_unique(
        "Local LM Studio (:1234)".to_string(),
        "http://127.0.0.1:1234/v1".to_string(),
        None,
        None,
        None,
    );
    push_unique(
        "Local Ollama (:11434)".to_string(),
        "http://localhost:11434/v1".to_string(),
        None,
        None,
        None,
    );
    push_unique(
        "Local llama.cpp (:8080)".to_string(),
        "http://localhost:8080/v1".to_string(),
        None,
        None,
        None,
    );
    push_unique(
        "Bundled FunctionGemma 270M (:8787)".to_string(),
        format!("http://127.0.0.1:{}/v1", SERVER_PORT),
        Some("functiongemma-270m-it".to_string()),
        Some(FUNCTIONGEMMA_CTX_32K),
        None,
    );

    out
}

/// Probe a single OpenAI-compatible `/v1/models` endpoint with a short timeout.
pub async fn probe_openai_endpoint(
    endpoint: &str,
    provider: &str,
    api_key: Option<&str>,
    preferred_model: Option<&str>,
    context_hint: Option<usize>,
    profile_name: Option<String>,
    timeout: Duration,
) -> Option<ScoutedEndpoint> {
    if crate::config::endpoint_has_userinfo(endpoint) {
        return None;
    }
    let mut builder = reqwest::Client::builder().timeout(timeout);
    if crate::config::is_local_endpoint(endpoint) {
        builder = builder.no_proxy();
    }
    let client = builder.build().ok()?;
    let url = format!("{}/models", endpoint.trim_end_matches('/'));
    let start = Instant::now();
    let req = crate::config::authorize_request(
        client.get(&url),
        endpoint,
        api_key.map(str::trim).filter(|k| !k.is_empty()),
    )
    .ok()?;
    let resp = req.send().await.ok()?;
    if !resp.status().is_success() {
        return None;
    }
    let body: serde_json::Value = resp.json().await.ok()?;
    let latency_ms = start.elapsed().as_millis() as u64;

    let models = body.get("data").and_then(|d| d.as_array())?;
    if models.is_empty() {
        return None;
    }

    let chosen_entry = preferred_model
        .and_then(|pref| {
            models.iter().find(|m| {
                m.get("id")
                    .and_then(|v| v.as_str())
                    .is_some_and(|id| id.eq_ignore_ascii_case(pref))
            })
        })
        .or_else(|| models.first())?;

    let model_id = chosen_entry.get("id")?.as_str()?.to_string();
    let reported_ctx = chosen_entry
        .get("max_model_len")
        .and_then(|v| v.as_u64())
        .or_else(|| chosen_entry.get("context_length").and_then(|v| v.as_u64()))
        .or_else(|| {
            chosen_entry
                .get("meta")
                .and_then(|m| m.get("n_ctx").or_else(|| m.get("n_ctx_train")))
                .and_then(|v| v.as_u64())
        })
        .filter(|&n| n > 0)
        .map(|n| n as usize);

    let context_length = reported_ctx
        .or(context_hint)
        .or_else(|| crate::config::match_profile(&model_id).and_then(|p| p.context_length))
        .unwrap_or(crate::config::UNKNOWN_MODEL_CONTEXT_LENGTH);

    Some(ScoutedEndpoint {
        provider: provider.to_string(),
        endpoint: endpoint.to_string(),
        model: model_id,
        context_length,
        latency_ms,
        profile_name,
    })
}

/// Build the grounded prompt sent to FunctionGemma 270M IT to select the best
/// live endpoint from `candidates`.
pub fn build_functiongemma_scout_prompt(
    failed_endpoint: &str,
    preferred_model: &str,
    candidates: &[ScoutedEndpoint],
) -> serde_json::Value {
    let mut listing = String::new();
    for (idx, c) in candidates.iter().enumerate() {
        listing.push_str(&format!(
            "[{}] provider={} | endpoint={} | model={} | context_length={} | latency_ms={}\n",
            idx,
            c.provider,
            crate::config::redact_endpoint(&c.endpoint),
            c.model,
            c.context_length,
            c.latency_ms
        ));
    }

    let system = "You are FunctionGemma 270M, Selfware's local endpoint recovery assistant.\n\
        The primary LLM endpoint is down. You must choose the best verified reachable fallback endpoint from the numbered candidate list.\n\
        Selection rules:\n\
        1. Prefer an external/LAN server with a larger coding model (e.g. qwen, deepseek, llama, lfm3) and larger context_length over the tiny boot-assistant model.\n\
        2. Among equals, prefer lower latency_ms.\n\
        3. Respond ONLY with `SELECT [<index>]` followed by a one-sentence reason.";

    let user = format!(
        "Failed endpoint: {}\nPreferred model: {}\n\nVerified live candidates:\n{}\nWhich candidate index `[0..{}]` should Selfware switch to?",
        crate::config::redact_endpoint(failed_endpoint),
        preferred_model,
        listing.trim_end(),
        candidates.len().saturating_sub(1)
    );

    let mut body = serde_json::json!({
        "model": SERVER_ALIAS,
        "messages": [
            {"role": "system", "content": system},
            {"role": "user", "content": user}
        ],
        "temperature": 0.0,
        "max_tokens": 96,
        "stream": false
    });
    crate::safety::redact::redact_json(&mut body);
    body
}

/// Parse FunctionGemma's reply and extract a valid index `< candidate_count`.
pub fn parse_functiongemma_selected_index(reply: &str, candidate_count: usize) -> Option<usize> {
    if candidate_count == 0 {
        return None;
    }
    // 1. Look for bracketed `[N]` first
    let bytes = reply.as_bytes();
    for i in 0..bytes.len() {
        if bytes[i] == b'[' {
            if let Some(end_rel) = bytes[i + 1..].iter().position(|&b| b == b']') {
                let inner = &reply[i + 1..i + 1 + end_rel].trim();
                if let Ok(idx) = inner.parse::<usize>() {
                    if idx < candidate_count {
                        return Some(idx);
                    }
                }
            }
        }
    }
    // 2. Fallback: first standalone integer token `< candidate_count`
    for token in reply.split(|c: char| !c.is_ascii_digit()) {
        if !token.is_empty() {
            if let Ok(idx) = token.parse::<usize>() {
                if idx < candidate_count {
                    return Some(idx);
                }
            }
        }
    }
    None
}

/// Deterministic fallback ranking when FunctionGemma is unavailable or its output
/// is ambiguous: prefers non-boot coding models with larger context windows and
/// lower latency, falling back to the local FunctionGemma boot server last.
pub fn rank_best_candidate_index(
    candidates: &[ScoutedEndpoint],
    preferred_model: &str,
) -> Option<usize> {
    if candidates.is_empty() {
        return None;
    }
    candidates
        .iter()
        .enumerate()
        .max_by_key(|(_, c)| {
            let is_boot = crate::boot::model::is_boot_server_endpoint(&c.endpoint)
                && (c.model.eq_ignore_ascii_case(SERVER_ALIAS)
                    || c.model.to_ascii_lowercase().contains("functiongemma"));
            let exact_model_match = c.model.eq_ignore_ascii_case(preferred_model);
            let has_profile = c.profile_name.is_some();
            (
                !is_boot as u8,
                exact_model_match as u8,
                has_profile as u8,
                c.context_length,
                std::cmp::Reverse(c.latency_ms),
            )
        })
        .map(|(idx, _)| idx)
}

/// Try to ensure the local FunctionGemma 270M (`boot-assistant`) server on port
/// `8787` is running if its binary and model are already present on disk (or if
/// `allow_download` is true).
pub async fn ensure_functiongemma_scout_server(allow_download: bool) -> Option<String> {
    if is_boot_server_healthy(SERVER_PORT).await {
        return Some(format!("http://127.0.0.1:{}/v1", SERVER_PORT));
    }
    let has_local_assets = find_llama_server().is_some()
        && (verify_model_file(&model_path_in(&boot_dir())).unwrap_or(false)
            || find_bundled_model().is_some());
    if !has_local_assets && !allow_download {
        return None;
    }
    ensure_background_boot_server_for_model_with_context(
        Some("functiongemma-270m-it"),
        Some(FUNCTIONGEMMA_CTX_32K),
    )
    .await
    .ok()
}

/// Ask a live FunctionGemma 270M server at `gemma_base_url` to choose the best
/// candidate index from `reachable`. Returns `Some((index, raw_reply))` when
/// FunctionGemma responds with a valid index `< reachable.len()`.
pub async fn select_candidate_with_functiongemma(
    gemma_base_url: &str,
    failed_endpoint: &str,
    preferred_model: &str,
    reachable: &[ScoutedEndpoint],
) -> Option<(usize, String)> {
    if reachable.is_empty() {
        return None;
    }
    let client = reqwest::Client::builder()
        .no_proxy()
        .timeout(Duration::from_secs(10))
        .build()
        .ok()?;
    let body = build_functiongemma_scout_prompt(failed_endpoint, preferred_model, reachable);
    let reply = super::chat::ask_once(&client, gemma_base_url, &body)
        .await
        .ok()?;
    let idx = parse_functiongemma_selected_index(&reply, reachable.len())?;
    Some((idx, reply))
}

/// Run the full FunctionGemma Endpoint Scout:
/// 1. Optionally boot/connect to local FunctionGemma 270M IT (`127.0.0.1:8787`).
/// 2. Probe all candidate endpoints for `config` concurrently with a fast timeout.
/// 3. Ask FunctionGemma 270M to select the best live endpoint (with deterministic
///    fallback ranking if FunctionGemma is not installed or returns no index).
pub async fn scout_and_select_endpoint(
    config: &crate::config::Config,
    allow_boot_spawn: bool,
) -> EndpointScoutReport {
    let env_gemma = std::env::var("SELFWARE_FUNCTIONGEMMA_ENDPOINT")
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    scout_and_select_endpoint_with_gemma_url(config, allow_boot_spawn, env_gemma).await
}

/// Inner implementation of [`scout_and_select_endpoint`] accepting an optional
/// explicit FunctionGemma base URL override (used by `SELFWARE_FUNCTIONGEMMA_ENDPOINT`
/// and hermetic unit tests).
pub async fn scout_and_select_endpoint_with_gemma_url(
    config: &crate::config::Config,
    allow_boot_spawn: bool,
    gemma_url_override: Option<String>,
) -> EndpointScoutReport {
    let gemma_url = if let Some(explicit_url) = gemma_url_override {
        Some(explicit_url)
    } else if allow_boot_spawn {
        let allow_dl = std::env::var("SELFWARE_AUTO_DOWNLOAD_FUNCTIONGEMMA")
            .ok()
            .is_some_and(|v| matches!(v.trim(), "1" | "true" | "yes" | "on"));
        ensure_functiongemma_scout_server(allow_dl).await
    } else if is_boot_server_healthy(SERVER_PORT).await {
        Some(format!("http://127.0.0.1:{}/v1", SERVER_PORT))
    } else {
        None
    };

    let candidates = candidate_endpoints_for_config(config);
    let probe_timeout = Duration::from_millis(1800);

    let probe_futures =
        candidates
            .into_iter()
            .map(|(provider, endpoint, pref_model, ctx_hint, profile_name)| {
                // Never leak the primary endpoint's API key to an unrelated server.
                // Only use a profile's own key or SELFWARE_LOCAL_API_KEY for local endpoints.
                let candidate_key: Option<String> = profile_name
                    .as_deref()
                    .and_then(|name| config.models.get(name))
                    .and_then(|p| p.api_key.as_ref().map(|k| k.expose().to_string()))
                    .or_else(|| {
                        if crate::config::is_local_endpoint(&endpoint) {
                            std::env::var("SELFWARE_LOCAL_API_KEY")
                                .ok()
                                .filter(|k| !k.is_empty())
                        } else {
                            None
                        }
                    });

                async move {
                    probe_openai_endpoint(
                        &endpoint,
                        &provider,
                        candidate_key.as_deref(),
                        pref_model.as_deref(),
                        ctx_hint,
                        profile_name,
                        probe_timeout,
                    )
                    .await
                }
            });

    let mut reachable: Vec<ScoutedEndpoint> = futures::future::join_all(probe_futures)
        .await
        .into_iter()
        .flatten()
        .collect();

    // If FunctionGemma is running on a dynamic boot port (8787..=8797) and wasn't
    // already in `reachable`, probe it via `/models` before including it as the
    // last-resort offline fallback (honest status + measured latency).
    if let Some(ref url) = gemma_url {
        let norm = url.trim_end_matches('/');
        if !reachable
            .iter()
            .any(|r| r.endpoint.trim_end_matches('/').eq_ignore_ascii_case(norm))
            && !config
                .endpoint
                .trim_end_matches('/')
                .eq_ignore_ascii_case(norm)
            && crate::boot::model::is_boot_server_endpoint(norm)
        {
            if let Some(hit) = probe_openai_endpoint(
                url,
                "Bundled FunctionGemma 270M",
                None,
                Some("functiongemma-270m-it"),
                Some(FUNCTIONGEMMA_CTX_32K),
                None,
                probe_timeout,
            )
            .await
            {
                reachable.push(hit);
            }
        }
    }

    if reachable.is_empty() {
        return EndpointScoutReport {
            failed_endpoint: config.endpoint.clone(),
            reachable,
            selected_index: None,
            functiongemma_used: false,
            functiongemma_rationale: None,
        };
    }

    // Consult FunctionGemma 270M IT when online!
    let mut functiongemma_used = false;
    let mut functiongemma_rationale = None;
    let mut selected_index = None;

    if let Some(ref base_url) = gemma_url {
        if let Some((idx, reply)) = select_candidate_with_functiongemma(
            base_url,
            &config.endpoint,
            &config.model,
            &reachable,
        )
        .await
        {
            functiongemma_used = true;
            functiongemma_rationale = Some(reply);
            selected_index = Some(idx);
        }
    }

    if selected_index.is_none() {
        selected_index = rank_best_candidate_index(&reachable, &config.model);
    }

    EndpointScoutReport {
        failed_endpoint: config.endpoint.clone(),
        reachable,
        selected_index,
        functiongemma_used,
        functiongemma_rationale,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{Config, ModelProfile, RedactedString};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    #[test]
    fn test_is_endpoint_down_error_detects_tunnel_and_network_outages() {
        assert!(is_endpoint_down_error(
            "HTTP 404: The endpoint ivo.ngrok.dev is offline. ERR_NGROK_3200"
        ));
        assert!(is_endpoint_down_error("ERR_NGROK_8012: upstream refused"));
        assert!(is_endpoint_down_error("502 Bad Gateway from cloudflare"));
        assert!(is_endpoint_down_error("503 Service Unavailable"));
        assert!(is_endpoint_down_error(
            "error trying to connect: tcp connect error: Connection refused (os error 61)"
        ));
        assert!(is_endpoint_down_error(
            "dns error: failed to lookup address information"
        ));
        assert!(!is_endpoint_down_error(
            "400 Bad Request: invalid parameter"
        ));
        assert!(!is_endpoint_down_error("Rate limit exceeded (429)"));
    }

    #[test]
    fn test_candidate_endpoints_for_config_includes_profiles_and_sibling_test_server() {
        let mut config = Config {
            endpoint: "https://ivo.ngrok.dev/v1".to_string(),
            model: "qwen3.8-27b".to_string(),
            context_length: 524288,
            ..Config::default()
        };
        config.models.insert(
            "lan".to_string(),
            ModelProfile {
                endpoint: "http://192.168.1.224:8000/v1".to_string(),
                model: "qwen3.8-27b".to_string(),
                api_key: None,
                max_tokens: 262144,
                temperature: 0.7,
                context_length: 524288,
                extra_body: None,
                ..Default::default()
            },
        );

        let candidates = candidate_endpoints_for_config(&config);
        // Should not include the failed primary endpoint itself
        assert!(!candidates
            .iter()
            .any(|(_, ep, _, _, _)| ep.contains("ivo.ngrok.dev")));
        // First candidate should be the configured `[models.lan]` profile, deduplicated with the sibling
        assert_eq!(candidates[0].1, "http://192.168.1.224:8000/v1");
        assert_eq!(candidates[0].4.as_deref(), Some("lan"));
        assert_eq!(
            candidates
                .iter()
                .filter(|(_, ep, _, _, _)| ep.contains("192.168.1.224"))
                .count(),
            1
        );
        // Should also include bundled FunctionGemma `:8787`
        assert!(candidates
            .iter()
            .any(|(_, ep, _, _, _)| ep.contains("127.0.0.1:8787")));
    }

    #[test]
    fn test_parse_functiongemma_selected_index_and_ranking() {
        assert_eq!(
            parse_functiongemma_selected_index(
                "SELECT [1] Choose LAN vLLM server serving qwen3.8-27b with 524k context.",
                3
            ),
            Some(1)
        );
        assert_eq!(
            parse_functiongemma_selected_index("Candidate 2 is fastest", 3),
            Some(2)
        );
        // Out-of-bounds index is rejected
        assert_eq!(parse_functiongemma_selected_index("SELECT [9]", 3), None);
        assert_eq!(parse_functiongemma_selected_index("SELECT [0]", 0), None);

        let candidates = vec![
            ScoutedEndpoint {
                provider: "Bundled FunctionGemma 270M".to_string(),
                endpoint: "http://127.0.0.1:8787/v1".to_string(),
                model: "boot-assistant".to_string(),
                context_length: 32768,
                latency_ms: 2,
                profile_name: None,
            },
            ScoutedEndpoint {
                provider: "LAN Test Server".to_string(),
                endpoint: "http://192.168.1.224:8000/v1".to_string(),
                model: "qwen3.8-27b".to_string(),
                context_length: 524288,
                latency_ms: 15,
                profile_name: Some("lan".to_string()),
            },
        ];
        // Deterministic ranker prefers the coding model (`qwen3.8-27b`) over `boot-assistant`
        assert_eq!(
            rank_best_candidate_index(&candidates, "qwen3.8-27b"),
            Some(1)
        );
    }

    #[tokio::test]
    async fn test_scout_and_select_endpoint_with_mock_functiongemma_and_fallback_server() {
        // 1. Spawn a mock live fallback OpenAI server serving `qwen3.8-27b`
        let fallback_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let fallback_addr = fallback_listener.local_addr().unwrap();
        tokio::spawn(async move {
            loop {
                let Ok((mut stream, _)) = fallback_listener.accept().await else {
                    break;
                };
                tokio::spawn(async move {
                    let mut buf = [0u8; 2048];
                    let _ = stream.read(&mut buf).await;
                    let body = r#"{"data":[{"id":"qwen3.8-27b","max_model_len":524288}]}"#;
                    let resp = format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                        body.len(),
                        body
                    );
                    let _ = stream.write_all(resp.as_bytes()).await;
                });
            }
        });

        // 2. Spawn a mock FunctionGemma 270M server that replies `SELECT [0]`
        let gemma_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let gemma_addr = gemma_listener.local_addr().unwrap();
        tokio::spawn(async move {
            loop {
                let Ok((mut stream, _)) = gemma_listener.accept().await else {
                    break;
                };
                tokio::spawn(async move {
                    let mut buf = [0u8; 4096];
                    let _ = stream.read(&mut buf).await;
                    let body = r#"{"choices":[{"message":{"content":"SELECT [0] Live LAN test server with qwen3.8-27b and 524k context."}}]}"#;
                    let resp = format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                        body.len(),
                        body
                    );
                    let _ = stream.write_all(resp.as_bytes()).await;
                });
            }
        });

        let fallback_endpoint = format!("http://{}/v1", fallback_addr);
        let gemma_endpoint = format!("http://{}/v1", gemma_addr);

        let mut config = Config {
            endpoint: "http://127.0.0.1:9/v1".to_string(), // Dead primary endpoint
            model: "qwen3.8-27b".to_string(),
            api_key: Some(RedactedString::new("sk-hosted-secret-must-not-leak")),
            ..Config::default()
        };
        config.models.insert(
            "backup".to_string(),
            ModelProfile {
                endpoint: fallback_endpoint.clone(),
                model: "qwen3.8-27b".to_string(),
                api_key: None,
                max_tokens: 262144,
                temperature: 0.7,
                context_length: 524288,
                extra_body: None,
                ..Default::default()
            },
        );

        let report =
            scout_and_select_endpoint_with_gemma_url(&config, false, Some(gemma_endpoint)).await;
        assert!(report.functiongemma_used);
        let chosen = report
            .selected()
            .expect("should select live backup endpoint");
        assert_eq!(chosen.endpoint, fallback_endpoint);
        assert_eq!(chosen.model, "qwen3.8-27b");
        assert_eq!(chosen.context_length, 524288);
        assert_eq!(chosen.profile_name.as_deref(), Some("backup"));
        assert!(report
            .summary_line()
            .contains("via FunctionGemma 270M scout"));
    }
}
