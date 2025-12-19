use crate::intent::types::{LongHorizonIntent, IntentId, IntentStatus};
use crate::kernel::time::Tick;
use crate::kernel::state::{StateDelta, SharedState};

use serde::{Deserialize, Serialize};

// Pure context for the planner
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IntentContext {
    pub active_focus: Option<String>,
    pub strength: f32,
}

pub struct LongHorizonIntentManager {
    // We don't store state here in the manager itself to keep it functional/stateless-ish logic wise,
    // but the pattern used in Reactor is that sidecars CAN have state. 
    // However, the Plan says `active_intents` is in `SharedState`. 
    // So the Manager is a logic operator on that state.
    // BUT Reactor needs to call `tick` which might need local tracking?
    // Let's keep it pure-ish. The Manager helps compute the Delta.
}

impl LongHorizonIntentManager {
    pub fn new() -> Self {
        Self {}
    }

    /// Primary cycle: Checks active intents in state, applies decay.
    /// Emits `IntentUpdate` deltas.
    pub fn tick(&self, _current_tick: Tick, state: &SharedState) -> Vec<StateDelta> {
        let mut deltas = Vec::new();
        
        for (_id, intent) in &state.active_intents {
            if intent.status == IntentStatus::Dissolved {
                continue; // Ignore dead ones (cleanup logic elsewhere?)
            }

            // 1. Apply Decay
            // Decay depends on status.
            let decay_mult = match intent.status {
                IntentStatus::Active => 1.0,
                // Suspended decays 2x faster
                IntentStatus::Suspended => 2.0,
                IntentStatus::Dissolved => 0.0,
            };
            
            let new_confidence = intent.confidence * (1.0 - (intent.decay_rate * decay_mult));
            
            // Check dissolution threshold
            if new_confidence < 0.1 {
                if intent.status != IntentStatus::Dissolved {
                    // Dissolve
                    let mut new_intent = intent.clone();
                    new_intent.status = IntentStatus::Dissolved;
                    new_intent.confidence = new_confidence;
                    deltas.push(StateDelta::IntentUpdate { intent: new_intent });
                }
            } else if (intent.confidence - new_confidence).abs() > 0.001 {
                // Emit update if changed significantly
                let mut new_intent = intent.clone();
                new_intent.confidence = new_confidence;
                deltas.push(StateDelta::IntentUpdate { intent: new_intent });
            }
        }
        
        deltas
    }

    /// Handles explicit signals (e.g. from user input) to reinforce or suspend.
    /// Returns delta.
    pub fn handle_reinforcement(&self, id: IntentId, state: &SharedState, current_tick: Tick) -> Option<StateDelta> {
         if let Some(intent) = state.active_intents.get(&id) {
             let mut new_intent = intent.clone();
             new_intent.confidence = (new_intent.confidence + 0.2).min(1.0);
             new_intent.last_reinforced = current_tick;
             new_intent.status = IntentStatus::Active; // Revive if suspended
             return Some(StateDelta::IntentUpdate { intent: new_intent });
         }
         None
    }

    pub fn handle_interruption(&self, state: &SharedState) -> Vec<StateDelta> {
        // Suspend ALL active intents
        let mut deltas = Vec::new();
        for (_id, intent) in &state.active_intents {
            if intent.status == IntentStatus::Active {
                let mut new_intent = intent.clone();
                new_intent.status = IntentStatus::Suspended;
                // Immediate penalty?
                new_intent.confidence *= 0.9;
                deltas.push(StateDelta::IntentUpdate { intent: new_intent });
            }
        }
        deltas
    }
    
    /// Create a new intent (Registration).
    /// Initial confidence is LOW (0.5).
    pub fn register_goal(&self, description: String, tick: Tick) -> StateDelta {
        let id = IntentId::new();
        let intent = LongHorizonIntent {
            id,
            description,
            confidence: 0.5, // Starts humble
            decay_rate: 0.01, // 1% per tick? TBD tuning
            status: IntentStatus::Active,
            created_at: tick,
            last_reinforced: tick,
        };
        StateDelta::IntentUpdate { intent }
    }

    /// View for Planner
    pub fn get_context(&self, state: &SharedState) -> IntentContext {
        // Find highest confidence Active intent
        let best = state.active_intents.values()
            .filter(|i| i.status == IntentStatus::Active)
            .max_by(|a, b| a.confidence.partial_cmp(&b.confidence).unwrap_or(std::cmp::Ordering::Equal));
            
        if let Some(i) = best {
            // Only if confidence is decent
            if i.confidence > 0.3 {
                 return IntentContext {
                     active_focus: Some(i.description.clone()),
                     strength: i.confidence,
                 };
            }
        }
        
        IntentContext {
            active_focus: None,
            strength: 0.0,
        }
    }
}
