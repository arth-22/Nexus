use crate::memory::types::{Claim, MemoryCandidate, Provenance, EntityId, Predicate, ClaimValue, Modality};
use crate::kernel::event::{Output, OutputStatus};
use crate::kernel::crystallizer::SymbolicSnapshot; // We need to share this visibility
use crate::kernel::latent::{LatentSlot, Modality as LatentModality};

/// The Memory Observer acts as a sensor, detecting potential memory candidates
/// from the stream of cognition (Outputs, Latents, User Inputs).
/// Invariant: It DOES NOT decide promotion. It only captures candidates.
pub struct MemoryObserver {
    candidates_buffer: Vec<MemoryCandidate>,
}

impl MemoryObserver {
    pub fn new() -> Self {
        Self {
            candidates_buffer: Vec::new(),
        }
    }

    /// Observe a realized output (Crystallization).
    pub fn observe_crystallization(&mut self, output: &Output, snapshot: &SymbolicSnapshot, current_tick: u64) {
        // Only interested in Committed outputs (Soft or Hard)
        // Drafts/Cancels are ignored by memory (Test 1: No Memory Spam)
        let strength = match output.status {
            OutputStatus::HardCommit => 1.0,
            OutputStatus::SoftCommit => 0.5,
            _ => return,
        };

        // Extract claims from the snapshot that generated this output
        for snapshot_claim in &snapshot.claims {
             // Heuristic mapping from Text Claim to Structured Claim
             // In a real system, an LLM or parser would structured this.
             // For Phase 7, we will wrap the text content.
             
             let claim = Claim::new(
                 EntityId::System, // Self-attribution for now? Or depends on content.
                 Predicate::Custom("stated".to_string()),
                 ClaimValue::Text(snapshot_claim.content.clone()),
                 Modality::Asserted,
             );

             self.candidates_buffer.push(MemoryCandidate {
                 content: claim,
                 strength: strength * snapshot_claim.confidence,
                 evidence_weight: 1, // Single observation
                 provenance: Provenance::System,
                 timestamp_tick: current_tick,
             });
        }
    }

    /// Observe a new latent state update.
    pub fn observe_latent(&mut self, slot: &LatentSlot, current_tick: u64) {
        // Filter for stable latents
        if slot.confidence < 0.7 {
            return;
        }

        let (predicate, object) = match slot.modality {
            LatentModality::Visual => (Predicate::Context, ClaimValue::Text("Visual stability detected".to_string())),
            LatentModality::Audio => (Predicate::Context, ClaimValue::Text("Audio activity detected".to_string())),
            LatentModality::Text => (Predicate::Custom("thinking".to_string()), ClaimValue::Text("Internal thought".to_string())),
        };

        // Latents are "Observed" modality
        let claim = Claim::new(
            EntityId::System,
            predicate,
            object,
            Modality::Observed,
        );

        self.candidates_buffer.push(MemoryCandidate {
            content: claim,
            strength: slot.confidence * 0.5, // Latents are weaker than text
            evidence_weight: 1,
            provenance: Provenance::System, // Internal perception
            timestamp_tick: current_tick,
        });
    }

    /// Explicit user input observation would go here (e.g. observe_input)
    /// to capture "My name is X" directly from input events.

    /// Drain the buffer of candidates.
    pub fn flush(&mut self) -> Vec<MemoryCandidate> {
        std::mem::take(&mut self.candidates_buffer)
    }
}
