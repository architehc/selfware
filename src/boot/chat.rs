//! `selfware boot --chat`: freeform setup Q&A with the tiny local model.
//!
//! The model is grounded by a system prompt that pins it to setup topics and
//! — critically — tells it that CONFIGS COME FROM RECIPE CARDS, so it never
//! invents endpoints or TOML. A top-1 keyword snippet from the repo's `docs/`
//! is added as retrieval context.

use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use super::model;

/// Grounding system prompt. The last rule is the safety-critical one: a raw
/// Qwen3-0.6B hallucinates configs (wrong endpoints, invalid TOML, wildcard
/// allowed_paths), so it must defer to the deterministic card path.
pub const GROUNDING_PROMPT: &str = "You are the selfware boot assistant, helping a user set up and recover their selfware installation.\n\
Topics you answer: choosing an LLM backend (OpenRouter, Ollama, vLLM, SGLang, LM Studio), API keys, endpoints, context sizes, and common setup errors.\n\
Rules:\n\
- Keep answers short and practical.\n\
- Configurations come from selfware's verified recipe cards, never from you: point the user to `selfware boot` (guided setup) instead of inventing endpoints, model ids, or TOML.\n\
- Always tell the user to verify a setup with `selfware llm-doctor`.\n\
- If a DOCUMENTATION SNIPPET is provided, prefer it over your own knowledge; if you don't know, say so.";

const MAX_SNIPPET_CHARS: usize = 1200;

/// Tiny stopword list — enough to keep "how do I" from dominating the score.
const STOPWORDS: &[&str] = &[
    "what", "how", "does", "do", "the", "and", "for", "with", "that", "this", "should", "can",
    "selfware", "when", "where", "why", "are", "is", "to", "a", "an", "i",
];

/// Extract lowercase keyword tokens (len >= 4, alphanumeric, not stopwords).
pub fn keywords(text: &str) -> Vec<String> {
    let mut out: Vec<String> = text
        .split(|c: char| !c.is_alphanumeric())
        .map(|t| t.to_ascii_lowercase())
        .filter(|t| t.len() >= 4 && !STOPWORDS.contains(&t.as_str()))
        .collect();
    out.sort();
    out.dedup();
    out
}

/// Where to look for markdown docs: `docs/` under the current directory
/// first (running from a checkout), then the compile-time repo path.
fn docs_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    let cwd_docs = PathBuf::from("docs");
    if cwd_docs.is_dir() {
        dirs.push(cwd_docs);
    }
    let manifest_docs = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("docs");
    if manifest_docs.is_dir() && !dirs.contains(&manifest_docs) {
        dirs.push(manifest_docs);
    }
    dirs
}

fn collect_markdown(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_markdown(&path, out);
        } else if path.extension().and_then(|e| e.to_str()) == Some("md") {
            out.push(path);
        }
    }
}

/// Keyword retrieval over markdown docs: split every file into paragraphs,
/// score by DISTINCT keyword hits, return the best paragraph (capped at
/// [`MAX_SNIPPET_CHARS`]). `None` when nothing matches — the chat then runs
/// on the grounding prompt alone.
pub fn retrieve_snippet(docs_dir: &Path, question: &str) -> Option<String> {
    let keys = keywords(question);
    if keys.is_empty() {
        return None;
    }
    let mut files = Vec::new();
    collect_markdown(docs_dir, &mut files);
    let mut best: Option<(usize, String)> = None;
    for file in files {
        let Ok(content) = std::fs::read_to_string(&file) else {
            continue;
        };
        for para in content.split("\n\n") {
            let lower = para.to_ascii_lowercase();
            let score = keys.iter().filter(|k| lower.contains(k.as_str())).count();
            if score == 0 {
                continue;
            }
            if best
                .as_ref()
                .is_none_or(|(best_score, _)| score > *best_score)
            {
                let snippet: String = para.trim().chars().take(MAX_SNIPPET_CHARS).collect();
                best = Some((score, snippet));
            }
        }
    }
    best.map(|(_, snippet)| snippet)
}

/// Chat-completion messages: grounding system prompt (+ retrieval snippet
/// when found), then the user's question.
pub fn build_messages(question: &str, snippet: Option<&str>) -> serde_json::Value {
    let system = match snippet {
        Some(s) => format!("{}\n\nDOCUMENTATION SNIPPET:\n{}", GROUNDING_PROMPT, s),
        None => GROUNDING_PROMPT.to_string(),
    };
    serde_json::json!({
        "model": model::SERVER_ALIAS,
        "messages": [
            {"role": "system", "content": system},
            {"role": "user", "content": question},
        ],
        "temperature": 0.7,
        "max_tokens": 512,
        "stream": false,
    })
}

/// One round-trip against the boot-assistant server; returns the reply text.
pub async fn ask_once(
    client: &reqwest::Client,
    base_url: &str,
    body: &serde_json::Value,
) -> Result<String> {
    let url = format!("{}/chat/completions", base_url.trim_end_matches('/'));
    let resp = client.post(&url).json(body).send().await?;
    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        anyhow::bail!("boot assistant returned HTTP {}: {}", status, text);
    }
    let json: serde_json::Value = resp.json().await?;
    json.get("choices")
        .and_then(|c| c.as_array())
        .and_then(|arr| arr.first())
        .and_then(|choice| choice.get("message"))
        .and_then(|msg| msg.get("content"))
        .and_then(|c| c.as_str())
        .map(|s| s.trim().to_string())
        .context("boot assistant reply had no message content")
}

/// `selfware boot --chat`: ensure the model, boot the server, REPL.
pub async fn run_boot_chat() -> Result<()> {
    let Some(server_bin) = model::find_llama_server() else {
        // Clean exit with the pointer — the chat is optional, never an error.
        println!("  {}", model::LLAMA_INSTALL_HINT);
        return Ok(());
    };
    let model_path = model::ensure_model(&model::boot_dir()).await?;

    println!("  Booting llama-server for the boot assistant...");
    let server = model::spawn_server(&server_bin, &model_path, model::SERVER_PORT).await?;
    println!(
        "  Boot assistant ready ({}). Ask setup questions; `exit` to quit.",
        server.base_url()
    );
    println!("  Configs come from recipe cards — run `selfware boot` for guided setup.\n");

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(120))
        .build()?;
    let stdin = std::io::stdin();
    let mut line = String::new();
    loop {
        print!("boot> ");
        std::io::stdout().flush()?;
        line.clear();
        if stdin.lock().read_line(&mut line)? == 0 {
            break; // EOF
        }
        let question = line.trim();
        if question.is_empty() {
            continue;
        }
        if matches!(question, "exit" | "quit" | ":q") {
            break;
        }
        let snippet = docs_dirs()
            .iter()
            .find_map(|dir| retrieve_snippet(dir, question));
        let body = build_messages(question, snippet.as_deref());
        match ask_once(&client, &server.base_url(), &body).await {
            Ok(reply) => println!("\n{}\n", reply),
            Err(e) => println!("\n  (boot assistant error: {:#})\n", e),
        }
    }
    println!("  Shutting down the boot assistant.");
    Ok(())
}

#[cfg(test)]
#[path = "../../tests/unit/boot/chat_test.rs"]
mod tests;
