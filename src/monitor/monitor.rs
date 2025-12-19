use crate::monitor::types::SelfObservation;
use crate::kernel::event::{InputEvent, InputContent};
use crate::kernel::state::{StateDelta, MetaLatents, SharedState};

/// The Monitor is a passive sidecar that observes the stream of events.
/// It maintains internal counters/scores and emits `MetaLatentUpdate` deltas.
pub struct SelfObservationMonitor {
    // Accumulators for meta-latents (before they are emitted/decayed)
    interruption_score: f32,
    confidence_penalty: f32,
    correction_score: f32,
    
    last_tick: u64,
}

impl SelfObservationMonitor {
    pub fn new() -> Self {
        Self {
            interruption_score: 0.0,
            confidence_penalty: 0.0,
            correction_score: 0.0,
            last_tick: 0,
        }
    }

    /// Primary cycle: Observe events, update scores, apply decay, emit delta.
    /// Invariant: MetaLatents decay towards 0.0 over time (Recovery).
    pub fn tick(&mut self, current_tick: u64, incoming_events: &[SelfObservation]) -> Option<StateDelta> {
        let elapsed = current_tick.saturating_sub(self.last_tick).max(1); // avoid 0
        self.last_tick = current_tick;

        // 1. Process Observations
        for obs in incoming_events {
            match obs {
                SelfObservation::UnexpectedInterruption { .. } => {
                    // Boost interruption sensitivity
                    self.interruption_score = (self.interruption_score + 0.3).min(1.0);
                }
                SelfObservation::UserCorrection { .. } => {
                    // Strong penalty
                    self.confidence_penalty = (self.confidence_penalty + 0.5).min(1.0);
                    self.correction_score = (self.correction_score + 0.4).min(1.0);
                }
                SelfObservation::ResponseTruncation { .. } => {
                     self.interruption_score = (self.interruption_score + 0.1).min(1.0);
                }
                SelfObservation::ConfidenceMismatch { .. } => {
                    self.confidence_penalty = (self.confidence_penalty + 0.2).min(1.0);
                }
                SelfObservation::StableAlignment | SelfObservation::Confirmation => {
                    // Verify recovery test?
                    // System heals faster on success
                    self.confidence_penalty = (self.confidence_penalty - 0.1).max(0.0);
                    self.interruption_score = (self.interruption_score - 0.05).max(0.0);
                }
            }
        }

        // 2. Apply Decay (Healing)
        // Decay rate depends on the parameter.
        // Interruption sensitivity decays fast (1s?). Confidence penalty decays slow.
        // Assuming Tick = 100ms.
        let decay_factor = 0.01 * (elapsed as f32); 
        
        self.interruption_score = (self.interruption_score - decay_factor).max(0.0);
        self.confidence_penalty = (self.confidence_penalty - (decay_factor * 0.5)).max(0.0); // Slower decay
        self.correction_score = (self.correction_score - decay_factor).max(0.0);

        // 3. Emit Delta
        // We always emit the current state so the kernel is in sync.
        // Optimization: only emit if changed significantly? 
        // For strictness, let's emit.
        
        Some(StateDelta::MetaLatentUpdate {
            delta: MetaLatents {
                interruption_sensitivity: self.interruption_score,
                confidence_penalty: self.confidence_penalty,
                correction_bias: self.correction_score,
            }
        })
    }

    /// Helper to detect observations from raw kernel events
    pub fn observe_raw(&self, _input: &InputEvent, _state: &SharedState) -> Vec<SelfObservation> {
        // This requires logic to match Input against previous Output.
        // For Phase 8 simple verification, we'll feed observations manually in tests,
        // or implement basic heuristics here later.
        // E.g. If Input is "No", emit UserCorrection.
        
        // Implementation for Test 1 (Unprompted Correction checks logic)
        // Logic:
        // If user input contains "No" or "Wrong", emit UserCorrection.
        // If user input starts while system is speaking (active outputs), emit Interruption.
        
        let mut obs = Vec::new();
        
        if let InputContent::Text(text) = &_input.content {
            if text.to_lowercase().contains("no") || text.to_lowercase().contains("wrong") {
                 obs.push(SelfObservation::UserCorrection { output_id: None });
            }
            if text.to_lowercase().contains("stop") {
                 obs.push(SelfObservation::UnexpectedInterruption { output_id: None });
            }
        }
        
        obs
    }
}
