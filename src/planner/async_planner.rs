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
    current_task: Option<tokio::task::JoinHandle<()>>,
}

impl AsyncPlanner {
    pub fn new(tx: mpsc::Sender<Event>) -> Self {
        Self {
            client: reqwest::Client::builder()
                .timeout(std::time::Duration::from_millis(TIMEOUT_MS))
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
            
            match client.post(LLM_URL).json(&body).send().await {
                Ok(resp) => {
                    if let Ok(text) = resp.text().await {
                        let intent: Option<Intent> = serde_json::from_str(&text).ok();
                         let parsed = intent.unwrap_or(Intent::DoNothing);
                         let _ = tx.send(Event::PlanProposed(epoch, parsed)).await;
                    }
                }
                Err(e) => {
                    warn!("LLM Plan Failed/Timeout: {}", e);
                    let _ = tx.send(Event::PlanProposed(epoch, Intent::DoNothing)).await;
                }
            }
        });
        
        self.current_task = Some(handle);
    }
}
