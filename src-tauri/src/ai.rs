//! AI subsystem (v2 W3 — docs/10).
//!
//! A pluggable, **BYO-key** provider that speaks the OpenAI Chat Completions
//! API, so it works with OpenAI, OpenRouter, Azure OpenAI, and local servers
//! (Ollama / LM Studio) by varying `base_url` + `model`. Context (the active
//! file + project facts) is assembled by the caller and passed as a system
//! message, so answers are grounded in the user's actual code.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
}

#[derive(Serialize)]
struct ApiRequest<'a> {
    model: &'a str,
    messages: &'a [ChatMessage],
    stream: bool,
}

#[derive(Deserialize)]
struct ApiResponse {
    choices: Vec<Choice>,
}
#[derive(Deserialize)]
struct Choice {
    message: RespMsg,
}
#[derive(Deserialize)]
struct RespMsg {
    content: String,
}

/// Send a chat completion and return the assistant message text.
pub async fn chat(
    base_url: &str,
    api_key: &str,
    model: &str,
    messages: Vec<ChatMessage>,
) -> Result<String, String> {
    if model.trim().is_empty() {
        return Err("No AI model configured (Settings → AI).".into());
    }
    let url = format!("{}/chat/completions", base_url.trim_end_matches('/'));
    let client = reqwest::Client::new();
    let mut req = client.post(&url).json(&ApiRequest {
        model,
        messages: &messages,
        stream: false,
    });
    if !api_key.is_empty() {
        req = req.bearer_auth(api_key);
    }
    let resp = req.send().await.map_err(|e| e.to_string())?;
    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(format!("AI provider error {status}: {body}"));
    }
    let parsed: ApiResponse = resp.json().await.map_err(|e| e.to_string())?;
    Ok(parsed
        .choices
        .into_iter()
        .next()
        .map(|c| c.message.content)
        .unwrap_or_default())
}
