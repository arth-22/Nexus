use super::state::StateDelta;
use super::event::InputEvent; // Assuming cancellation cmds come as inputs for now
use std::collections::HashSet;

#[derive(Debug, Default)]
pub struct CancellationRegistry {
    // In Phase 0 kernel, this might just track IDs that *should* be canceled
    // Actual Tokio handles would live in the Effect layer (Reactor), not here
    pending_cancels: HashSet<String>,
}

impl CancellationRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Pure function: Observe inputs -> Decide what to cancel -> Return Deltas
    pub fn process(&mut self, inputs: &[InputEvent]) -> Vec<StateDelta> {
        let mut deltas = Vec::new();
        
        for input in inputs {
            // Stub logic: if input is "STOP", cancel everything (Phase 0 simplification)
            if input.content.trim().eq_ignore_ascii_case("STOP") {
                 // In a real system, we'd parse the target ID. 
                 // Here we just emit a generic "root_task" cancel for demo
                 deltas.push(StateDelta::TaskCanceled("root_task".to_string()));
            }
        }
        
        deltas
    }
}
