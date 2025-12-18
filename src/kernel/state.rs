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

#[derive(Debug, Clone)]
pub struct SharedState {
    // Private fields to enforce encapsulation
    beliefs: HashMap<String, f32>,
    active_outputs: HashMap<OutputId, Output>,
    // In strict model, we might track canceled task IDs or just effects
    canceled_tasks: HashSet<String>,
    // Monotonic version for Epoch validation
    pub version: u64,
}

impl Default for SharedState {
    fn default() -> Self {
        Self {
            beliefs: HashMap::new(),
            active_outputs: HashMap::new(),
            canceled_tasks: HashSet::new(),
            version: 0,
        }
    }
}

use crate::kernel::time::Tick;

impl SharedState {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn snapshot(&self, tick: Tick) -> crate::planner::types::StateSnapshot {
        crate::planner::types::StateSnapshot {
            epoch: crate::planner::types::PlanningEpoch {
                tick,
                state_version: self.version,
            },
            last_input_ticks: 0, // TODO: Track this real
            user_active: false, // Stub
            active_outputs: self.active_outputs.len(),
            recent_interruptions: self.canceled_tasks.len(),
        }
    }

    /// Pure reduction: State + Delta -> Mutated State
    pub fn reduce(&mut self, delta: StateDelta) {
        self.version += 1;
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
