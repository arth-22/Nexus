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
    VisualStateUpdate { hash: u64, stability: f32 },
    Tick(Tick),
}

#[derive(Debug, Clone)]
pub struct VisualState {
    pub hash: u64,
    pub stability_score: f32, // 0.0 - 1.0
    // Phase 4: pub already_shown: HashSet<String>
}

impl Default for VisualState {
    fn default() -> Self {
        Self {
            hash: 0,
            stability_score: 0.0,
        }
    }
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
    
    // Audio / Control State
    pub last_tick: Tick,
    pub user_speaking: bool,
    pub turn_pressure: f32, // 0.0 - 1.0
    pub last_speech_start: Option<Tick>,
    pub last_speech_end: Option<Tick>,
    pub hesitation_detected: bool,
    
    // Vision State
    pub visual: VisualState,
}

impl Default for SharedState {
    fn default() -> Self {
        Self {
            beliefs: HashMap::new(),
            active_outputs: HashMap::new(),
            canceled_tasks: HashSet::new(),
            version: 0,
            last_tick: Tick { frame: 0 },
            user_speaking: false,
            turn_pressure: 0.0,
            last_speech_start: None,
            last_speech_end: None,
            hesitation_detected: false,
            visual: VisualState::default(), 
        }
    }
}

// Ensure Tick is used
use crate::kernel::time::Tick;
use crate::kernel::event::{InputContent, AudioSignal};

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
            last_input_ticks: 0, // Placeholder
            user_active: self.user_speaking,
            active_outputs: self.active_outputs.len(),
            recent_interruptions: self.canceled_tasks.len(),
        }
    }

    /// Pure reduction: State + Delta -> Mutated State
    pub fn reduce(&mut self, delta: StateDelta) {
        // Version increments on mutation (except maybe Tick?)
        // Let's increment on everything for safety.
        self.version += 1;
        
        match delta {
            StateDelta::Tick(t) => {
                self.last_tick = t;
                // Turn Pressure Dynamics
                // Decay if not speaking
                if !self.user_speaking {
                    self.turn_pressure = (self.turn_pressure - 0.01).max(0.0);
                } else {
                    // If speaking and system has active outputs (interruption)
                    if !self.active_outputs.is_empty() {
                         self.turn_pressure = (self.turn_pressure + 0.1).min(1.0);
                    }
                }
                
                // Visual Stability Decay (Physics)
                // If no update received this tick, decay slightly
                self.visual.stability_score = (self.visual.stability_score - 0.01).max(0.0);
            }
            StateDelta::InputReceived(input) => {
                match input.content {
                    InputContent::Audio(AudioSignal::SpeechStart) => {
                        self.user_speaking = true;
                        self.last_speech_start = Some(self.last_tick);
                        self.hesitation_detected = false; 
                    }
                    InputContent::Audio(AudioSignal::SpeechEnd) => {
                        self.user_speaking = false;
                        self.last_speech_end = Some(self.last_tick);
                        
                        // Check Hesitation (Short burst < 10 ticks = 200ms)
                        if let Some(start) = self.last_speech_start {
                            // Tick should support subtraction or frame diff
                            if self.last_tick.frame >= start.frame {
                                let duration = self.last_tick.frame - start.frame;
                                if duration < 10 && duration > 0 {
                                    self.hesitation_detected = true;
                                }
                            }
                        }
                    }
                    _ => {}
                }
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
            StateDelta::VisualStateUpdate { hash, stability } => {
                self.visual.hash = hash;
                self.visual.stability_score = stability;
            }
        }
    }
    
    // Read-only accessors for Planner
    pub fn active_outputs(&self) -> &HashMap<OutputId, Output> {
        &self.active_outputs
    }
}
