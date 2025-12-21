use anyhow::{Result, anyhow};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use crate::kernel::speech::planner::SpeechIntent;
use std::time::Duration;

#[derive(Clone)]
pub struct LLMService {
    client: Client,
    base_url: String,
}

#[derive(Serialize)]
struct CompletionRequest {
    prompt: String,
    stream: bool,
    n_predict: usize,
    temperature: f32,
    stop: Vec<String>,
}

#[derive(Deserialize)]
struct CompletionResponse {
    content: String,
}

impl LLMService {
    pub fn new() -> Self {
        Self {
            client: Client::builder()
                .timeout(Duration::from_secs(2)) // HARD Timeout Enforcement (Network Level)
                .build()
                .unwrap_or_default(),
            base_url: "http://localhost:8080".to_string(),
        }
    }

    pub async fn generate_speech(&self, intent: SpeechIntent) -> Result<String> {
        let system_prompt = "You are a quiet, thinking cognitive companion. You respond briefly, neutrally, and precisely. You do not offer advice unless asked. You are calm.";
        
        let user_prompt = match intent {
            SpeechIntent::Clarification(context) => format!("The user's intent is ambiguous. Ask a neutral clarification question. Context: {}", context),
            SpeechIntent::Confirmation(details) => format!("Confirm this action briefly: {}", details),
            SpeechIntent::Offer(details) => format!("Offer these options neutrally: {}", details),
        };

        let full_prompt = format!("System: {}\nUser: {}\nAssistant:", system_prompt, user_prompt);

        let request_body = CompletionRequest {
            prompt: full_prompt,
            stream: false, // One-shot only
            n_predict: 64, // Strict Token Limit
            temperature: 0.4, // Strict Temperature
            stop: vec!["User:".to_string(), "System:".to_string()],
        };

        // We use the /completion endpoint of llama-server
        let response = self.client.post(format!("{}/completion", self.base_url))
            .json(&request_body)
            .send()
            .await?;

        if !response.status().is_success() {
             return Err(anyhow!("LLM Server Error: {}", response.status()));
        }

        let resp_json: CompletionResponse = response.json().await?;
        Ok(resp_json.content.trim().to_string())
    }
}
