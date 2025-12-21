use tokio::sync::mpsc;
use tracing::warn;
use serde_json::json;
use crate::kernel::event::Event;
use crate::planner::types::{StateSnapshot, Intent};

const LLM_URL: &str = "http://localhost:8080/completion";
const DEFAULT_TIMEOUT_MS: u64 = 200;

pub struct AsyncPlanner {
    client: reqwest::Client,
    tx: mpsc::Sender<Event>,
    current_task: Option<tokio::task::JoinHandle<()>>,
}

impl AsyncPlanner {
    pub fn new(tx: mpsc::Sender<Event>) -> Self {
        let timeout_ms = std::env::var("NEXUS_PLANNER_TIMEOUT_MS")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(DEFAULT_TIMEOUT_MS);

        Self {
            client: reqwest::Client::builder()
                .timeout(std::time::Duration::from_millis(timeout_ms))
                .build()
                .unwrap_or_else(|_| reqwest::Client::new()),
            tx,
            current_task: None,
        }
    }

    pub fn abort(&mut self) {
        if let Some(task) = self.current_task.take() {
            task.abort();
        }
    }

    pub fn dispatch(&mut self, snapshot: StateSnapshot) {
        // Abort any existing in-flight plan
        self.abort();

        let client = self.client.clone();
        let tx = self.tx.clone();
        let epoch = snapshot.epoch;

        let handle = tokio::spawn(async move {
            let prompt = format!(
                "STATE: {}\nAVAILABLE INTENTS: BeginResponse(confidence), Delay(ticks), AskClarification, DoNothing.\nReturn ONLY valid JSON.",
                serde_json::to_string(&snapshot).unwrap_or_default()
            );

            let body = json!({
                "prompt": prompt,
                "n_predict": 64,
                "json_schema": {
                    "type": "object",
                    "properties": {
                        "intent": { "type": "string", "enum": ["BeginResponse", "Delay", "AskClarification", "DoNothing"] },
                        "data": { "type": "object" }
                    }
                }
            });
            

            
            println!("[AsyncPlanner] Sending Request to LLM...");
            match client.post(LLM_URL).json(&body).send().await {
                Ok(resp) => {
                    if let Ok(text) = resp.text().await {
                        // Debug Log Raw Response for Config Verification
                        println!("[AsyncPlanner] Raw LLM Response (Epoch {:?}): {}", epoch, text);

                        // Parse the wrapper JSON from llama-server
                        let json_resp: serde_json::Value = serde_json::from_str(&text).unwrap_or_default();
                        let content = json_resp["content"].as_str().unwrap_or(text.as_str()); // Fallback to raw if logic changes

                        println!("[AsyncPlanner] Extracted Content: {}", content);

                        let intent: Option<Intent> = serde_json::from_str(content).ok();
                         let parsed = intent.unwrap_or(Intent::DoNothing);
                         println!("[AsyncPlanner] Parsed Intent: {:?}", parsed);
                         let _ = tx.send(Event::PlanProposed(epoch, parsed)).await;
                    }
                }
                Err(e) => {
                    println!("[AsyncPlanner] LLM Plan Failed/Timeout: {}", e);
                    let _ = tx.send(Event::PlanProposed(epoch, Intent::DoNothing)).await;
                }
            }
        });
        
        self.current_task = Some(handle);
    }
}
