use crate::kernel::crystallizer::{SymbolicSnapshot, CrystallizationDecision};

/// PURE FUNCTION: Converts a symbolic snapshot into text based on the decision.
pub fn realize(snapshot: &SymbolicSnapshot, decision: &CrystallizationDecision) -> String {
    // Phase 6: Template-based realization
    
    // 1. Concat claims
    let content = snapshot.claims.iter()
        .map(|c| c.content.clone())
        .collect::<Vec<_>>()
        .join(" ");
        
    if content.is_empty() {
        // Phase D Verification: Default speech
        return "This is a long verification message to test the interruptibility of the Nexus system. Please speak now to test the kill switch.".to_string(); 
    }
    
    match decision {
        CrystallizationDecision::AllowPartial => {
            // Hedge
            format!("It seems that {}...", content)
        }
        CrystallizationDecision::AllowHard => {
            // Direct
            format!("{}.", content)
        }
        _ => String::new(), // Should not happen if called correctly
    }
}
