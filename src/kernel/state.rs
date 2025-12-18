use super::event::{InputEvent, Output, OutputId, OutputStatus};
use std::collections::{HashMap, HashSet};

/// Strict state delta. This is the ONLY way state mutates.
#[derive(Debug, Clone)]
pub enum StateDelta {
    InputReceived(InputEvent),
    OutputProposed(Output),
    OutputCommitted(OutputId),
    OutputCanceled(OutputId),
    TaskCanceled(String),
}

#[derive(Debug, Clone, Default)]
pub struct SharedState {
    // Private fields to enforce encapsulation
    beliefs: HashMap<String, f32>,
    active_outputs: HashMap<OutputId, Output>,
    // In strict model, we might track canceled task IDs or just effects
    canceled_tasks: HashSet<String>,
}

impl SharedState {
    pub fn new() -> Self {
        Self::default()
    }

    /// Pure reduction: State + Delta -> Mutated State
    pub fn reduce(&mut self, delta: StateDelta) {
        match delta {
            StateDelta::InputReceived(_input) => {
                // For Phase 0 stub, we might just log or set a "recent input" flag
                // In real impl, this updates discourse state
            }
            StateDelta::OutputProposed(output) => {
                self.active_outputs.insert(output.id, output);
            }
            StateDelta::OutputCommitted(id) => {
                if let Some(out) = self.active_outputs.get_mut(&id) {
                    out.status = OutputStatus::Committed;
                }
            }
            StateDelta::OutputCanceled(id) => {
                if let Some(out) = self.active_outputs.get_mut(&id) {
                    out.status = OutputStatus::Canceled;
                }
            }
            StateDelta::TaskCanceled(task_id) => {
                self.canceled_tasks.insert(task_id.clone());
                // Cascade: Cancel all outputs belonging to this task
                for out in self.active_outputs.values_mut() {
                    if let Some(pid) = &out.parent_id {
                        if pid == &task_id {
                            out.status = OutputStatus::Canceled;
                        }
                    }
                }
            }
        }
    }
    
    // Read-only accessors for Planner
    pub fn active_outputs(&self) -> &HashMap<OutputId, Output> {
        &self.active_outputs
    }
}
