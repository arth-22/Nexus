use serde::{Deserialize, Serialize};

use crate::kernel::time::Tick;
use crate::kernel::state::{SharedState, StateDelta};
use crate::kernel::intent::types::{IntentCandidate, IntentHypothesis};
use std::collections::HashMap;
use crate::kernel::telemetry::recorder::TelemetryRecorder;
use crate::kernel::telemetry::event::TelemetryEvent;

pub type IntentId = String;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum IntentStatus {
    Active,
    Suspended,
    Dormant,
    Completed,
    Invalidated,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LongHorizonIntent {
    pub id: IntentId,
    pub hypothesis: IntentHypothesis,
    pub source_symbol_ids: Vec<String>,
    pub created_at: Tick,
    pub last_active_at: Tick,
    pub last_updated_at: Tick, // For decay calculation
    pub suspended_at: Option<Tick>,
    pub decay_score: f32,        // 1.0 -> 0.0
    pub status: IntentStatus,
}

// Config Constants
const DECAY_RATE_PER_TICK: f32 = 0.9997; // Very slow decay
const DORMANCY_THRESHOLD: f32 = 0.3;
const RESUME_THRESHOLD: f32 = 0.6; // Lower score, but context match boosts confidence
const INVALIDATION_THRESHOLD: f32 = 0.1; // Hard kill line

pub struct LongHorizonIntentManager {
    pub active_intents: HashMap<IntentId, LongHorizonIntent>,
}

impl LongHorizonIntentManager {
    pub fn new() -> Self {
        Self {
            active_intents: HashMap::new(),
        }
    }

    /// Register a Stable Phase G intent as a Long-Horizon Intent.
    /// If an equivalent intent is Suspended/Dormant, reinforce and resume it.
    /// Else create new.
    pub fn register_intent(&mut self, candidate: &IntentCandidate, _state: &SharedState, current_tick: Tick, telemetry: &mut TelemetryRecorder) -> Vec<StateDelta> {
        let mut deltas = Vec::new();
        // Check if we have an existing intent with same ID
        // Or if this is a "Reinforcement"
        
        let mut existing_found = false;
        
        // MVP: If ID matches, Reinforce. Else Create.
        if let Some(existing) = self.active_intents.get_mut(&candidate.id) {
            existing_found = true;
            existing.last_active_at = current_tick;
            existing.decay_score = 1.0; // Refresh
            existing.status = IntentStatus::Active;
            existing.last_updated_at = current_tick;
            
            deltas.push(StateDelta::LongHorizonIntentUpdate(existing.clone()));
            
            // TELEMETRY: Reinforced (Active -> Active) - Optional? 
            // IntentLifecycle tracks status changes.
            // Active -> Active is not a status change.
        }
        
        if !existing_found {
             let new_intent = LongHorizonIntent {
                id: candidate.id.clone(),
                hypothesis: candidate.hypothesis.clone(),
                source_symbol_ids: candidate.source_symbol_ids.clone(),
                created_at: current_tick,
                last_active_at: current_tick,
                last_updated_at: current_tick,
                suspended_at: None,
                decay_score: 1.0, // Fresh
                status: IntentStatus::Active,
            };
            self.active_intents.insert(new_intent.id.clone(), new_intent.clone());
            deltas.push(StateDelta::LongHorizonIntentUpdate(new_intent.clone()));
            
            // TELEMETRY: Created
            telemetry.record(TelemetryEvent::IntentLifecycle {
                intent_id: new_intent.id,
                from: IntentStatus::Invalidated, // Proxy for None
                to: IntentStatus::Active,
            });
        }
        
        deltas
    }

    /// Suspend an specific intent (safe).
    pub fn suspend_intent(&mut self, id: &IntentId, _state: &SharedState, current_tick: Tick, telemetry: &mut TelemetryRecorder) -> Option<StateDelta> {
         if let Some(intent) = self.active_intents.get_mut(id) {
             if intent.status == IntentStatus::Active {
                 let old_status = intent.status.clone();
                 intent.status = IntentStatus::Suspended;
                 intent.suspended_at = Some(current_tick);
                 intent.decay_score *= 0.8; // Immediate penalty for interruption
                 intent.last_updated_at = current_tick;
                 
                 // TELEMETRY: Suspended
                 telemetry.record(TelemetryEvent::IntentLifecycle {
                    intent_id: id.clone(),
                    from: old_status,
                    to: IntentStatus::Suspended,
                 });

                 return Some(StateDelta::LongHorizonIntentUpdate(intent.clone()));
             }
         }
         None
    }

    /// Suspend ALL active intents (Interruption Supremacy).
    pub fn handle_interruption(&mut self, state: &SharedState, current_tick: Tick, telemetry: &mut TelemetryRecorder) -> Vec<StateDelta> {
         let mut deltas = Vec::new();
         // Suspend ALL Active intents
         // Need to collect IDs first to avoid borrow issues
         let active_ids: Vec<String> = self.active_intents.values()
             .filter(|i| i.status == IntentStatus::Active)
             .map(|i| i.id.clone())
             .collect();
             
         for id in active_ids {
             if let Some(d) = self.suspend_intent(&id, state, current_tick, telemetry) {
                 deltas.push(d);
             }
         }
         
         deltas
    }

    /// Attempt to Resume a Suspended intent based on context.
    /// Resumption predicates:
    /// - Status {Suspended, Dormant}
    /// - Decay > RESUME_THRESHOLD
    /// - No conflicting Active intent
    /// - Context Match (Symbol Overlap OR Planner Request)
    pub fn try_resume(&mut self, state: &SharedState, current_tick: Tick, telemetry: &mut TelemetryRecorder) -> Vec<StateDelta> {
        let mut deltas = Vec::new();
        
        // Look for "Forming" intents in current context
        // If they match a Suspended/Dormant intent, Resume it.
        
        if let crate::kernel::intent::types::IntentState::Forming(candidates) = &state.intent_state {
             // Check over candidates
             for fc in candidates {
                 // Check if any Suspended intent matches this candidate's Symbols or Semantics
                 let mut match_found: Option<LongHorizonIntent> = None;
                 
                 for susp in self.active_intents.values() 
                     .filter(|i| i.status == IntentStatus::Suspended || i.status == IntentStatus::Dormant )
                 {
                     // Check symbol overlap
                     for s_id in &fc.source_symbol_ids {
                         if susp.source_symbol_ids.contains(s_id) {
                             match_found = Some(susp.clone()); // FIX: Use clone if copy not avail
                             break;
                         }
                     }
                     if match_found.is_some() { break; }
                 }
                 
                 if let Some(mut resumed) = match_found {
                      let old_status = resumed.status.clone();
                      let dormant_duration = match resumed.suspended_at {
                          Some(t) => current_tick.frame.saturating_sub(t.frame),
                          None => 0,
                      };

                      resumed.status = IntentStatus::Active;
                      resumed.suspended_at = None;
                      resumed.last_active_at = current_tick;
                      // Boost score slightly?
                      resumed.decay_score = (resumed.decay_score + 0.1).min(1.0);
                      resumed.last_updated_at = current_tick;
                      
                      deltas.push(StateDelta::LongHorizonIntentUpdate(resumed.clone()));
                      
                      // TELEMETRY: Resumption
                      telemetry.record(TelemetryEvent::IntentLifecycle {
                          intent_id: resumed.id.clone(),
                          from: old_status,
                          to: IntentStatus::Active,
                      });
                      telemetry.record(TelemetryEvent::IntentResumption {
                          intent_id: resumed.id.clone(),
                          dormant_ticks: dormant_duration,
                      });
                 }
             }
        }
        
        deltas
    }

    /// Apply Decay (Tick).
    /// Monotonic: score *= rate^delta
    pub fn tick(&mut self, current_tick: Tick, _state: &SharedState, telemetry: &mut TelemetryRecorder) -> Vec<StateDelta> {
        let mut deltas = Vec::new();
        
        // Apply Monotonic Decay
        // If intent.decay_score < Threshold -> Invalidate
        
        // We iterate over mutable refs? No, shared state is passed as Ref.
        // But active_intents is owned by self.
        
        let ids: Vec<String> = self.active_intents.keys().cloned().collect();
        
        for id in ids {
            if let Some(intent) = self.active_intents.get_mut(&id) {
                // Decay Logic
                if intent.status == IntentStatus::Invalidated {
                    continue;
                }

                // Delta from LAST UPDATE (Per-Intent)
                let delta = current_tick.frame.saturating_sub(intent.last_updated_at.frame) as f32;
                
                // Check if we should apply decay this tick.
                // "Active" intents decay? Yes, unless reinforced.
                // "Suspended" intents decay? Yes.
                
                let mut new_intent = intent.clone();
                if delta > 0.0 {
                    new_intent.decay_score *= DECAY_RATE_PER_TICK.powf(delta); // Apply delta-based decay
                    new_intent.last_updated_at = current_tick;
                }
                
                // Thresholds
                let old_status = new_intent.status.clone();
                let mut status_changed = false;

                if new_intent.decay_score < INVALIDATION_THRESHOLD {
                    new_intent.status = IntentStatus::Invalidated;
                    status_changed = true;
                } else if new_intent.decay_score < DORMANCY_THRESHOLD && new_intent.status == IntentStatus::Suspended {
                     // Suspended -> Dormant
                     new_intent.status = IntentStatus::Dormant;
                     status_changed = true;
                } else if new_intent.decay_score < DORMANCY_THRESHOLD && new_intent.status == IntentStatus::Active {
                     // Weak Active -> Dormant? Maybe? Or just Invalid logic.
                     // Active intents are usually reinforced by planner. If ignored, they fade.
                     new_intent.status = IntentStatus::Dormant;
                     status_changed = true;
                }
                
                if status_changed {
                    telemetry.record(TelemetryEvent::IntentLifecycle {
                        intent_id: new_intent.id.clone(),
                        from: old_status,
                        to: new_intent.status.clone(),
                    });
                }

                // Only emit if changed significantly? 
                // Or every tick? Every tick is too much traffic.
                // Emission: Only on status change or significant decay steps?
                // SharedState needs it to display? 
                // We return delta. SharedState applies it.
                // Optimization: Only return delta if diff > epsilon or status change.
                
                if (new_intent.decay_score - intent.decay_score).abs() > 0.001 || new_intent.status != intent.status {
                     *intent = new_intent.clone();
                     deltas.push(StateDelta::LongHorizonIntentUpdate(new_intent));
                }
            }
        }
        
        deltas
    }
}

// Planner Context View
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IntentContext {
    pub active_focus: Option<String>,
    pub strength: f32,
}

impl LongHorizonIntentManager {
    /// View for Planner
    pub fn get_context(&self, state: &SharedState) -> IntentContext {
        // Find highest decay_score Active intent
        // Using `decay_score` as "strength" proxy (combined with confidence?)
        // `LongHorizonIntent` has `decay_score`.
        // It doesn't store original confidence explicitly (legacy did).
        // Let's use `decay_score` as the strength of presence.
        
        let best = state.active_intents.values()
            .filter(|i| i.status == IntentStatus::Active)
            .max_by(|a, b| a.decay_score.partial_cmp(&b.decay_score).unwrap_or(std::cmp::Ordering::Equal));
            
        if let Some(i) = best {
            // Only if strong enough
            if i.decay_score > RESUME_THRESHOLD {
                 return IntentContext {
                     active_focus: Some(format!("{:?}", i.hypothesis)),
                     strength: i.decay_score,
                 };
            }
        }
        
        IntentContext {
            active_focus: None,
            strength: 0.0,
        }
    }
}
