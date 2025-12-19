use crate::kernel::state::{SharedState, StateDelta};
use crate::kernel::intent::types::{IntentCandidate, IntentStability};
use crate::kernel::memory::types::{MemoryCandidate, MemoryKey, MemoryRecord};
use crate::kernel::time::Tick;
use uuid::Uuid;
use crate::kernel::telemetry::recorder::TelemetryRecorder;
use crate::kernel::telemetry::event::{TelemetryEvent, MemoryEventKind};
// Assuming 50ms per tick
// Minimum window: 1 minute = 60s = 1200 ticks
const MIN_CONSOLIDATION_WINDOW: u64 = 1200; 

// Max candidate age: 10 minutes = 600s = 12000 ticks
const MAX_CANDIDATE_AGE: u64 = 12000;

// Decay config
const DECAY_FACTOR: f32 = 0.9995; // Slow decay per tick
const FORGET_THRESHOLD: f32 = 0.1;

pub struct MemoryConsolidator;

impl MemoryConsolidator {
    pub fn new() -> Self {
        Self
    }

    /// Process a Stable Intent to potentially create or reinforce a Memory Candidate.
    pub fn process_intent(&self, intent: &IntentCandidate, state: &SharedState, telemetry: &mut TelemetryRecorder) -> Vec<StateDelta> {
        // 1. Gate: Must be Stable and High Confidence
        if intent.stability != IntentStability::Stable || intent.confidence < 0.85 {
            return vec![];
        }

        let key = MemoryKey::from_intent(intent);
        let mut deltas = Vec::new();
        let current_tick = state.last_tick;

        // 2. Identity Match
        // Check if we already have a candidate with this semantic key
        let existing = state.memory_candidates.values().find(|c| c.key == key);

        if let Some(cand) = existing {
            // Reinforce
            deltas.push(StateDelta::MemoryCandidateReinforced(cand.id.clone(), current_tick));
            // TELEMETRY
            telemetry.record(TelemetryEvent::MemoryEvent {
                kind: MemoryEventKind::Reinforced,
                memory_id: cand.id.clone(),
            });
        } else {
            // New Candidate
            let id = Uuid::new_v4().to_string();
            let new_cand = MemoryCandidate {
                id: id.clone(),
                key,
                intent: intent.clone(),
                created_at: current_tick,
                reinforcement_count: 1, // First appearance counts as 1? Or 0? Let's say 1.
                last_reinforced_at: current_tick,
            };
            deltas.push(StateDelta::MemoryCandidateCreated(new_cand));
            
            // TELEMETRY
            telemetry.record(TelemetryEvent::MemoryEvent {
                kind: MemoryEventKind::CandidateCreated,
                memory_id: id,
            });
        }

        deltas
    }

    /// Run periodic maintenance: Promotion, Decay, Pruning.
    pub fn tick(&self, current_tick: Tick, state: &SharedState, telemetry: &mut TelemetryRecorder) -> Vec<StateDelta> {
        let mut deltas = Vec::new();

        // 1. Decay Long Term Memory
        for record in state.long_term_memory.values() {
            // Access slows decay? 
            // Model: Decay happens every tick relative to *time since last access*?
            // Or just decay continuously and access boosts strength back up?
            // User Refinement: "Access should slow decay, not increase strength."
            // Simple Model: Decay applies every tick. 
            // If recently accessed, maybe we skip decay?
            // "Access delay decay start" -> If (current - last_accessed) < GRACE_PERIOD, no decay.
            
            let time_since_access = current_tick.frame.saturating_sub(record.last_accessed_at.frame);
            if time_since_access > 200 { // 10 seconds grace
                let new_strength = record.strength * DECAY_FACTOR;
                if new_strength < FORGET_THRESHOLD {
                    deltas.push(StateDelta::MemoryForgotten(record.id.clone()));
                    telemetry.record(TelemetryEvent::MemoryEvent { 
                        kind: MemoryEventKind::Forgotten, 
                        memory_id: record.id.clone() 
                    });
                } else {
                    deltas.push(StateDelta::MemoryDecayed { 
                        id: record.id.clone(), 
                        new_strength 
                    });
                    // Optional: Don't log decay every tick, only significant ones?
                    // Or "Decayed" event is for significant drops?
                    // For now, let's NOT log MemoryDecayed every tick as it's high volume.
                    // User Plan: "Decay too aggressive" can be seen via Forgotten count or Intent stats.
                    // But Plan says "MemoryStats { decayed: u64 }".
                    // If we increment this every tick for every record, it will explode.
                    // Maybe only log if it drops below a tier (0.5, 0.2)?
                    // Or let's just log Forgotten for now to be safe on volume.
                    // Actually, let's log "Decayed" only if strength crosses 0.5 boundary downwards?
                    // Too complex. Let's just log Forgotten.
                    // The metrics struct has "decayed: u64". maybe we skip using it heavily.
                }
            }
        }

        // 2. Promote Candidates
        for cand in state.memory_candidates.values() {
            let age = current_tick.frame.saturating_sub(cand.created_at.frame);
            
            let mut should_promote = false;
            let mut ask_consent = false;

            // Basic Eligibility: Reinforcement >= 2 (Strict), Age >= MIN_WINDOW
            if cand.reinforcement_count >= 2 && age >= MIN_CONSOLIDATION_WINDOW {
                 // Check Consent
                 let consent_state = state.memory_consent.get(&cand.key)
                                     .map(|c| c.state)
                                     .unwrap_or(crate::kernel::memory::consent::MemoryConsentState::Unknown);

                 match consent_state {
                     crate::kernel::memory::consent::MemoryConsentState::Granted => {
                         should_promote = true;
                     }
                     crate::kernel::memory::consent::MemoryConsentState::Declined | crate::kernel::memory::consent::MemoryConsentState::Ignored => {
                         // Never promote
                     }
                     crate::kernel::memory::consent::MemoryConsentState::Unknown => {
                         // Strict Heuristic: Statement Only + Very High Confidence
                         let is_statement = matches!(cand.intent.hypothesis, crate::kernel::intent::types::IntentHypothesis::Statement); 
                         let high_confidence = cand.intent.confidence >= 0.95;
                         
                         if is_statement && high_confidence {
                             ask_consent = true;
                         }
                     }
                 }
            }

            if should_promote {
                // PROMOTE
                let record = MemoryRecord {
                    id: cand.id.clone(),
                    intent: cand.intent.clone(),
                    first_committed_at: current_tick,
                    last_accessed_at: current_tick,
                    strength: 0.5, // Initial strength
                };
                deltas.push(StateDelta::MemoryPromoted(record));
                deltas.push(StateDelta::MemoryCandidateRemoved(cand.id.clone()));
                
                telemetry.record(TelemetryEvent::MemoryEvent {
                    kind: MemoryEventKind::Promoted,
                    memory_id: cand.id.clone(),
                });
            } else if ask_consent {
                // Ask for consent, do NOT promote yet.
                // This delta will trigger the Scheduler/Reactor to emit the SideEffect.
                deltas.push(StateDelta::MemoryConsentAsked(cand.key.clone(), current_tick));
            }
            
            // Check Pruning (Separate from promotion decision)
            // Rule: Idle for too long without promotion
            let idle_time = current_tick.frame.saturating_sub(cand.last_reinforced_at.frame);
            if idle_time > MAX_CANDIDATE_AGE {
                deltas.push(StateDelta::MemoryCandidateRemoved(cand.id.clone()));
            }
        }

        deltas
    }
}
