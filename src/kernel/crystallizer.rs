use crate::kernel::state::SharedState;
use crate::kernel::time::Tick;

#[derive(Debug, Clone, PartialEq)]
pub enum CrystallizationDecision {
    Deny,
    Delay { ms: u64 },
    AllowPartial,
    AllowHard,
}

#[derive(Debug, Clone)]
pub struct Claim {
    pub content: String,
    pub confidence: f32,
    pub modality_support: Vec<String>, // "Vision", "Text"
}

#[derive(Debug, Clone)]
pub struct SymbolicSnapshot {
    pub claims: Vec<Claim>,
    pub base_uncertainty: f32,
    pub timestamp: Tick,
}

/// PURE FUNCTION: Decides if the system can crystallize thoughts into text.
/// No side effects.
pub fn check_gate(state: &SharedState) -> CrystallizationDecision {
    // 1. Hard Constraints
    if state.user_speaking {
        return CrystallizationDecision::Deny;
    }
    
    // Check if any task was recently canceled (simple heuristic)
    if !state.canceled_tasks().is_empty() {
        // In a real system we'd check timestamps. 
        // For strict phase 6, if we just got canceled, we should probably Deny or Delay.
        // But for now, let's assume `canceled_tasks` accumulates forever? 
        // No, we need to check recent interruptions via a timestamp if available.
        // `snapshot` uses `recent_interruptions` count.
        // Let's rely on `turn_pressure` or similar.
    }
    
    // 2. Soft Latents (Uncertainty)
    let uncertainty = state.latents.global_uncertainty();
    
    // Thresholds
    const DENY_THRESHOLD: f32 = 0.8;
    const PARTIAL_THRESHOLD: f32 = 0.4;
    
    if uncertainty > DENY_THRESHOLD {
        return CrystallizationDecision::Deny; // Too confused
    }
    
    // If somewhat uncertain, delay? 
    // Or allow partial.
    // Let's say if > 0.6, Delay.
    if uncertainty > 0.6 {
        return CrystallizationDecision::Delay { ms: 500 };
    }
    
    if uncertainty > PARTIAL_THRESHOLD {
        return CrystallizationDecision::AllowPartial;
    }
    
    CrystallizationDecision::AllowHard
}

/// Deterministic extraction of claims from state
pub fn extract_snapshot(state: &SharedState) -> SymbolicSnapshot {
    // For Phase 6, we stub this with a single claim based on Latents
    // In real system, this would cluster embeddings.
    
    let mut claims = Vec::new();
    
    // Scan Latents for clusters
    for slot in &state.latents.slots {
        use crate::kernel::latent::Modality;
        match slot.modality {
            Modality::Visual => {
                if slot.confidence > 0.8 {
                    claims.push(Claim {
                        content: "Visual context is stable.".to_string(),
                        confidence: slot.confidence,
                        modality_support: vec!["Vision".to_string()],
                    });
                }
            },
            Modality::Audio => {
                if slot.confidence > 0.8 {
                     claims.push(Claim {
                        content: "High energy audio detected.".to_string(),
                        confidence: slot.confidence,
                        modality_support: vec!["Audio".to_string()],
                    });
                }
            },
            Modality::Text => {
                if slot.confidence > 0.8 {
                     // Heuristic: Text latent means we have a strong thought
                     claims.push(Claim {
                        content: "Thought processed.".to_string(),
                        confidence: slot.confidence,
                        modality_support: vec!["Text".to_string()],
                    });
                }
            }
        }
    }
    
    SymbolicSnapshot {
        claims,
        base_uncertainty: state.latents.global_uncertainty(),
        timestamp: state.last_tick,
    }
}
