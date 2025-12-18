use serde::{Deserialize, Serialize};
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

/// Unique identifier for an entity (System, User, or a specific Topic).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum EntityId {
    System,
    User,
    Topic(String),
}

/// The nature of the relationship described in the claim.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Predicate {
    Prefers,  // User prefers X
    Is,       // A is B (fact)
    Knows,    // A knows B
    Capability, // System can do X
    Context,  // The current context is X
    Custom(String),
}

/// The content of the claim.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ClaimValue {
    Text(String),
    Boolean(bool),
    Number(f64),
    // Future: Structured(Value)
}

/// How this information was acquired.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Modality {
    Asserted, // Explicitly stated by user or hard-coded
    Inferred, // Deduced by the system (lower confidence)
    Observed, // Noticed from behavior (implicit)
}

/// The atomic unit of semantic memory.
/// Semi-structured to allow future conflict resolution.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Claim {
    pub subject: EntityId,
    pub predicate: Predicate,
    pub object: ClaimValue,
    pub modality: Modality,
}

impl Claim {
    /// Generates a stable hash for the (Subject + Predicate) pair.
    /// This allows O(1) detection of potential contradictions.
    /// E.g. (User, Prefers) -> Hash.
    pub fn key_hash(&self) -> u64 {
        let mut s = DefaultHasher::new();
        self.subject.hash(&mut s);
        self.predicate.hash(&mut s);
        // Note: We deliberately do NOT hash the object or modality.
        // We want to find *conflicting* claims about the same subject/predicate.
        s.finish()
    }

    pub fn new(subject: EntityId, predicate: Predicate, object: ClaimValue, modality: Modality) -> Self {
        Self { subject, predicate, object, modality }
    }
}

// Implement Hash manually for Claim if needed, or rely on auto-derive if we want full struct hashing.
// But for key_hash logic, the method above is specific.

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Provenance {
    System,
    User,
    Inferred,
}

/// A candidate memory emitted by the Observer.
/// Not yet a permanent memory; needs consolidation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MemoryCandidate {
    pub content: Claim,
    /// Determining how strong the initial signal is (0.0 to 1.0).
    pub strength: f32,
    /// Number of times this has been observed/reinforced in short term.
    pub evidence_weight: u32,
    pub provenance: Provenance,
    pub timestamp_tick: u64,
}

/// An entry in the Semantic Store (Long-term).
/// Append-only, versioned validation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SemanticMemoryEntry {
    pub id: String, // UUID
    pub claim: Claim,
    pub confidence: f32, // 0.0 to 1.0
    pub provenance: Provenance,
    pub created_at_tick: u64,
    pub last_accessed_tick: u64,
    pub version: u32,
    pub previous_version_id: Option<String>,
}

/// An entry in the Episodic Store (Session-scale).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EpisodicMemoryEntry {
    pub claim: Claim,
    pub confidence: f32,
    pub created_at_tick: u64,
    pub last_reinforced_tick: u64,
    /// Decays over time. If < 0, it is removed.
    pub decay_rate: f32, 
}
