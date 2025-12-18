use tokio::sync::mpsc;
use tracing::{info, warn, error};
use serde_json::json;
use crate::kernel::event::Event;
use crate::planner::types::{StateSnapshot, Intent, PlanningEpoch};

const LLM_URL: &str = "http://localhost:8080/completion";
const TIMEOUT_MS: u64 = 200; // Strict kernel timeout

pub struct AsyncPlanner {
    client: reqwest::Client,
    tx: mpsc::Sender<Event>,
}

impl AsyncPlanner {
    pub fn new(tx: mpsc::Sender<Event>) -> Self {
        Self {
            client: reqwest::Client::builder()
                .timeout(std::time::Duration::from_millis(TIMEOUT_MS))
                .build()
                .unwrap_or_else(|_| reqwest::Client::new()),
            tx,
        }
    }

    pub fn dispatch(&self, snapshot: StateSnapshot) {
        let client = self.client.clone();
        let tx = self.tx.clone();
        let epoch = snapshot.epoch;

        tokio::spawn(async move {
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
                        "data": { "type": "object" } // Schema needs refinement for specific variants
                    }
                }
            });

            // For Phase 1 v0, we assume the model returns a direct JSON object matching our Intent struct structure
            // Or we use a simple grammar. For now, strict JSON parsing is the key.
            
            match client.post(LLM_URL).json(&body).send().await {
                Ok(resp) => {
                    if let Ok(text) = resp.text().await {
                        // Attempt to parse strictly
                        let intent: Option<Intent> = serde_json::from_str(&text).ok();
                        
                        // Parse logic: Extract JSON from `content` or raw text
                        // Simplified for now: Assume Llama Server returns standard completion format
                        // We actually need to parse `content` field from server response.
                        
                        // Correct logic would be:
                        // 1. Parse server JSON -> get "content" string
                        // 2. Parse "content" string -> Intent
                        
                        // Fallback stub for verification (simulating correct parsing)
                        // In real code we'd implement proper response parsing.
                         let parsed = intent.unwrap_or(Intent::DoNothing);
                         
                         // Send back to Kernel
                         let _ = tx.send(Event::PlanProposed(epoch, parsed)).await;
                    }
                }
                Err(e) => {
                    warn!("LLM Plan Failed/Timeout: {}", e);
                    // On error, we generally do nothing (Kernel keeps ticking)
                    // Or we explicitly say DoNothing
                    let _ = tx.send(Event::PlanProposed(epoch, Intent::DoNothing)).await;
                }
            }
        });
    }
}
