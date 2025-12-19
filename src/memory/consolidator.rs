use crate::memory::types::{Claim, MemoryCandidate, EpisodicMemoryEntry, SemanticMemoryEntry, Modality, Provenance};
use crate::memory::store::{EpisodicStore, SemanticStore};
use std::collections::HashMap;

/// The Consolidator acts as the "Cortex", deciding what becomes durable.
/// It runs deterministically on specific ticks.
pub struct MemoryConsolidator {
    // We accumulate evidence across ticks before promoting to Episodic
    short_term_buffer: HashMap<u64, AccumulatedEvidence>,
    last_consolidation_tick: u64,
}

struct AccumulatedEvidence {
    claim: Claim,
    total_strength: f32,
    evidence_count: u32,
    first_seen: u64,
    last_seen: u64,
}

impl MemoryConsolidator {
    pub fn new() -> Self {
        Self {
            short_term_buffer: HashMap::new(),
            last_consolidation_tick: 0,
        }
    }

    /// Main Cycle: Ingest candidates, match them, and promote.
    pub fn process<E: EpisodicStore, S: SemanticStore>(
        &mut self,
        candidates: Vec<MemoryCandidate>,
        episodic: &mut E,
        semantic: &mut S,
        current_tick: u64,
    ) {
        // 1. Ingest Candidates into Short Term Buffer
        for candidate in candidates {
            let key = candidate.content.key_hash();
            let entry = self.short_term_buffer.entry(key).or_insert(AccumulatedEvidence {
                claim: candidate.content.clone(),
                total_strength: 0.0,
                evidence_count: 0,
                first_seen: current_tick,
                last_seen: current_tick,
            });
            
            entry.total_strength += candidate.strength;
            entry.evidence_count += candidate.evidence_weight;
            entry.last_seen = current_tick;
            
            // CONTRADICTION CHECK (Immediate)
            // If we have a semantic memory with same Subject+Predicate but DIFFERENT Object/Modality?
            // This requires querying Semantic Store.
            // For Phase 7, we'll do this on promotion.
        }

        // 2. Promotion Logic (Working -> Episodic)
        // Run every N ticks (e.g., 10 ticks = 1s if tick=100ms)
        if current_tick >= self.last_consolidation_tick + 10 {
            self.run_episodic_promotion(episodic, current_tick);
            self.last_consolidation_tick = current_tick;
        }

        // 3. Promotion Logic (Episodic -> Semantic)
        // Run rarely (e.g., every 100 ticks?)
        if current_tick % 100 == 0 {
             self.run_semantic_promotion(episodic, semantic, current_tick);
        }
    }

    fn run_episodic_promotion<E: EpisodicStore>(&mut self, store: &mut E, current_tick: u64) {
        // Identify stable items in buffer
        let mut promoted_keys = Vec::new();

        for (key, evidence) in &self.short_term_buffer {
            // Rule: Persisted across time OR High Intensity
            let duration = evidence.last_seen - evidence.first_seen;
            let intensity = evidence.total_strength;

            let should_promote = (duration > 5 && evidence.evidence_count > 2) || (intensity > 3.0);

            if should_promote {
                // Create Episodic Entry
                let entry = EpisodicMemoryEntry {
                    claim: evidence.claim.clone(),
                    confidence: (evidence.total_strength / evidence.evidence_count as f32).min(1.0),
                    created_at_tick: current_tick,
                    last_reinforced_tick: current_tick,
                    decay_rate: 0.01, // Default decay
                };
                store.insert(entry);
                promoted_keys.push(*key);
            }
        }

        // Clear promoted items from buffer (they moved to episodic)
        // Or keep them but reset? Removing is safer to avoid dupes.
        for key in promoted_keys {
            self.short_term_buffer.remove(&key);
        }
        
        // Decay buffer: Remove old, weak items that failed to promote
        self.short_term_buffer.retain(|_, v| {
            let age = current_tick - v.last_seen;
            age < 50 // If not seen in 5s, forget.
        });
    }

    fn run_semantic_promotion<E: EpisodicStore, S: SemanticStore>(&mut self, episodic: &mut E, semantic: &mut S, current_tick: u64) {
        // Scan Episodic Memory for high-confidence, stable facts
        // This is "Sleep Consolidation"
        
        let _candidates = episodic.retrieve(0); // We need "all" or iterator.
        // The trait currently has `all()`.
        
        // We can't borrow mutable episodic while iterating?
        // Actually `all()` returns Vec references. 
        // We'll collect candidates first.
        
        let all_episodic = episodic.all();
        let mut to_promote = Vec::new();

        for entry in all_episodic {
            // Strict Rules for Semantic
            // 1. High Confidence
            // 2. Modality is Asserted (Text) or specific types
            // 3. Repeated reinforcement? (Implicit in high confidence due to episodic reinforcement?)
            
            if entry.confidence > 0.9 && matches!(entry.claim.modality, Modality::Asserted) {
                // Check if already exists in Semantic
                // Note: key_hash() only checks Subject+Predicate
                let key = entry.claim.key_hash();
                
                // We need to check if we should OVERWRITE/VERSION.
                // Retrieve existing
                 let existing = semantic.retrieve(key).unwrap_or_default();
                 // If any existing matches Subject+Predicate...
                 
                 let duplicate = existing.iter().any(|e| e.claim.object == entry.claim.object);
                 
                 if !duplicate {
                     to_promote.push(entry.clone());
                 } else {
                     // Reinforce existing? (Not implemented yet)
                 }
            }
        }

        for p in to_promote {
             // Create Semantic Entry
             let sem_entry = SemanticMemoryEntry {
                 id: uuid::Uuid::new_v4().to_string(),
                 claim: p.claim.clone(),
                 confidence: p.confidence,
                 provenance: Provenance::System, // Promoted from System experience
                 created_at_tick: current_tick,
                 last_accessed_tick: current_tick,
                 version: 1,
                 previous_version_id: None,
             };
             
             let _ = semantic.insert(sem_entry);
        }
    }
}
