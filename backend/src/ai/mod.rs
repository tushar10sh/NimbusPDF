use anyhow::Context;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::path::Path;

/// Loaded from data/<user|session>/settings/ai_config.toml
#[derive(Clone, Deserialize)]
pub struct UserAiConfig {
    pub endpoint_url: String,
    pub model: String,
    pub api_key: Option<String>,
}

/// OpenAI-compatible chat request (works with Ollama too)
#[derive(Serialize)]
pub struct ChatCompletionRequest {
    pub model: String,
    pub messages: Vec<Message>,
    pub stream: bool,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct Message {
    pub role: String,
    pub content: String,
}

pub struct AiProxy {
    client: Client,
}

impl AiProxy {
    pub fn new() -> Self {
        Self {
            client: Client::new(),
        }
    }

    pub fn new_with_client(client: Client) -> Self {
        Self { client }
    }

    /// Send a non-streaming completion request and return the assistant message.
    pub async fn complete(
        &self,
        user_cfg: &UserAiConfig,
        system_prompt: &str,
        messages: Vec<Message>,
    ) -> anyhow::Result<String> {
        let mut all_messages = vec![Message {
            role: "system".into(),
            content: system_prompt.into(),
        }];
        all_messages.extend(messages);

        let req = ChatCompletionRequest {
            model: user_cfg.model.clone(),
            messages: all_messages,
            stream: false,
        };

        let mut builder = self.client.post(&user_cfg.endpoint_url).json(&req);
        if let Some(key) = &user_cfg.api_key {
            if !key.is_empty() {
                builder = builder.bearer_auth(key);
            }
        }

        let resp: serde_json::Value = builder.send().await?.json().await?;
        let content = resp["choices"][0]["message"]["content"]
            .as_str()
            .context("unexpected AI response shape")?
            .to_string();
        Ok(content)
    }
}

/// Load a prompt template from disk and substitute {document_context}.
pub fn load_prompt(prompt_path: &Path, document_context: &str) -> anyhow::Result<String> {
    let template = std::fs::read_to_string(prompt_path)
        .with_context(|| format!("reading prompt file {}", prompt_path.display()))?;
    Ok(template.replace("{document_context}", document_context))
}
