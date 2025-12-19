use crate::kernel::intent::types::{IntentCandidate, IntentHypothesis};
use crate::kernel::time::Tick;
use serde::{Serialize, Deserialize};
use std::hash::{Hash, Hasher};
use std::collections::hash_map::DefaultHasher;

pub type MemoryId = String;

/// Opaque key for semantic matching to prevent cross-topic reinforcement.
/// "Explain Gravity" != "Explain Taxes" even if both are Inquiries.
#[derive(Debug, Clone, Hash, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemoryKey {
    pub hypothesis: IntentHypothesis,
    pub symbol_fingerprint: u64, 
}

impl MemoryKey {
    pub fn from_intent(intent: &IntentCandidate) -> Self {
        // Use the Arbitrator-derived semantic hash for recurrence matching
        // This ensures "Explain Gravity" (T1) matches "Explain Gravity" (T2)
        // even if symbol IDs differ.
        Self {
            hypothesis: intent.hypothesis.clone(),
            symbol_fingerprint: intent.semantic_hash, 
        }
    }
}

// We will likely need to update IntentCandidate to support this. 
// For now, defining the types.

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryCandidate {
    pub id: MemoryId,
    pub key: MemoryKey,
    pub intent: IntentCandidate,
    pub created_at: Tick,
    pub reinforcement_count: u32,
    pub last_reinforced_at: Tick,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryRecord {
    pub id: MemoryId,
    pub intent: IntentCandidate,
    pub first_committed_at: Tick,
    pub last_accessed_at: Tick,
    pub strength: f32, // 0.0 - 1.0
}
