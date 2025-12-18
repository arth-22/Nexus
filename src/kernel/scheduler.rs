use super::event::{Intent, Output, OutputId, OutputStatus};
use super::state::StateDelta;
use super::time::Tick;


pub struct Scheduler;

pub enum SideEffect {
    SpawnAudio(OutputId, String),
    Log(String),
}

impl Scheduler {
    /// Pure Projection: Intent + Tick -> (StateDelta, SideEffect)
    /// This keeps the "Decision" (Delta) separate from "Effect" (IO)
    pub fn schedule(&self, intent: Intent, tick: Tick, ordinal: u16) -> (Option<StateDelta>, Option<SideEffect>) {
        // OutputId is deterministic: tick frame + ordinal index of intent
        let output_id = OutputId { tick: tick.frame, ordinal };

        match intent {
            Intent::Log { msg } => {
                // Log is just a side effect, no state change for now
                (None, Some(SideEffect::Log(msg)))
            }
            Intent::Say { text } => {
                // 1. Propose output to state
                let output = Output {
                    id: output_id,
                    content: text.clone(),
                    status: OutputStatus::Draft, // Starts as draft
                    proposed_at: tick,
                    committed_at: None,
                    parent_id: Some("root_task".to_string()), // Phase 0 stub: all outputs belong to root
                };
                
                let delta = StateDelta::OutputProposed(output);
                
                // 2. Queue side effect (rendering)
                // In a real system, we might wait for "Commit" before side-effecting.
                // For Phase 0 stub, we'll pretend we render the draft immediately or wait.
                // Let's say we render drafts.
                let effect = SideEffect::SpawnAudio(output_id, text);
                
                (Some(delta), Some(effect))
            }
        }
    }
}
