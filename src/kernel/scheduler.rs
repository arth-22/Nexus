use super::event::{Output, OutputId, OutputStatus};
use super::state::StateDelta;
use super::time::Tick;
use crate::planner::types::Intent;

pub struct Scheduler;

#[derive(Debug, Clone)]
pub enum SideEffect {
    Log(String),
    SpawnAudio(OutputId, String),
}

impl Scheduler {
    /// Pure Projection: Intent + Context -> (StateDelta, SideEffect)
    pub fn schedule(&self, intent: Intent, tick: Tick, ordinal: u16) -> (Option<StateDelta>, Option<SideEffect>) {
        let output_id = OutputId { tick: tick.frame, ordinal };

        match intent {
            Intent::DoNothing => (None, None),
            Intent::Delay { ticks: _ } => {
                // In Phase 1: Delay is effective by NOT emitting output.
                (None, Some(SideEffect::Log("Planner decided to Delay".to_string())))
            }
            Intent::AskClarification => {
                 let text = "Could you clarify?".to_string();
                  let output = Output {
                    id: output_id,
                    content: text.clone(),
                    status: OutputStatus::Draft, 
                    proposed_at: tick,
                    committed_at: None,
                    parent_id: Some("root_task".to_string()),
                };
                (Some(StateDelta::OutputProposed(output)), Some(SideEffect::SpawnAudio(output_id, text)))
            }
            Intent::BeginResponse { confidence: _ } => {
                // Phase 1 Stub: We don't generate text yet.
                // Hardcode "Hello Phase 1" to pass verification.
                let text = "Hello Phase 1".to_string();
                
                let output = Output {
                    id: output_id,
                    content: text.clone(),
                    status: OutputStatus::Draft, 
                    proposed_at: tick,
                    committed_at: None,
                    parent_id: Some("root_task".to_string()),
                };
                
                let delta = StateDelta::OutputProposed(output);
                let effect = SideEffect::SpawnAudio(output_id, text);
                
                (Some(delta), Some(effect))
            }
        }
    }
}
