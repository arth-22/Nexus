use crate::memory::types::{Claim, MemoryCandidate, EpisodicMemoryEntry, SemanticMemoryEntry, Modality, Provenance};
use crate::memory::store::{EpisodicStore, SemanticStore};
use crate::kernel::time::Tick;
use std::cmp::Ordering;

/// Evidence retrieved from memory for the Planner.
#[derive(Debug, Clone, PartialEq)]
pub struct MemoryEvidence {
    pub content: Claim,
    pub confidence: f32,
    pub source: RetrievalSource,
    pub relevance: f32,
    pub recency_tick: u64,
}

#[derive(Debug, Clone, PartialEq)]
pub enum RetrievalSource {
    Episodic,
    Semantic,
}

/// The Retriever handles queries from the Planner/Cognition.
/// Invariant: Memory never "pushes" into the planner.
pub struct MemoryRetriever;

impl MemoryRetriever {
    /// Retrieve evidence relevant to a query.
    /// Query hash is currently (Subject + Predicate).
    pub fn retrieve<E: EpisodicStore, S: SemanticStore>(
        query_hash: u64,
        episodic: &E,
        semantic: &S,
    ) -> Vec<MemoryEvidence> {
        let mut results = Vec::new();

        // 1. Check Episodic (Recent context)
        let episodic_hits = episodic.retrieve(query_hash);
        for entry in episodic_hits {
             results.push(MemoryEvidence {
                 content: entry.claim.clone(),
                 confidence: entry.confidence,
                 source: RetrievalSource::Episodic,
                 relevance: 1.0, // Exact match on hash
                 recency_tick: entry.last_reinforced_tick,
             });
        }

        // 2. Check Semantic (Long term facts)
        if let Ok(semantic_hits) = semantic.retrieve(query_hash) {
             for entry in semantic_hits {
                 results.push(MemoryEvidence {
                     content: entry.claim.clone(),
                     confidence: entry.confidence,
                     source: RetrievalSource::Semantic,
                     relevance: 1.0,
                     recency_tick: entry.last_accessed_tick,
                 });
             }
        }

        // 3. Sort by Score (Confidence * Relevance * Decay?)
        // For now, strict sort by Confidence.
        results.sort_by(|a, b| b.confidence.partial_cmp(&a.confidence).unwrap_or(Ordering::Equal));
        
        results
    }
}
