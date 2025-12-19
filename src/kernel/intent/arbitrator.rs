use super::types::*;
use uuid::Uuid;

pub struct IntentArbitrator;

impl IntentArbitrator {
    pub fn new() -> Self {
        Self
    }

    /// Assess the incoming text and source symbol to update the IntentState.
    /// 
    /// Refinement 1: If current state is Suspended, we guard against overwrite unless verified.
    /// For MVP, we pass the *current state* context loosely or just logic inside.
    /// 
    /// heuristics:
    /// - "?" -> Inquiry
    /// - "turn off", "play" -> Command
    /// - "um", "maybe" -> ThinkingAloud / Fragment
    /// - Short length -> Fragment
    pub fn assess(&self, text: &str, symbol_id: &str, current_state: &IntentState) -> IntentState {
        // Refinement 1: Suspended Protection
        if let IntentState::Suspended(existing) = current_state {
            // For MVP: If new text is short/noise, keep Suspended.
            // If it seems substantive, we break out of Suspension into Forming.
            if text.len() < 5 {
                return IntentState::Suspended(existing.clone());
            }
            // Else, fall through to re-assessment (Reinforcement)
        }

        let text_lower = text.to_lowercase();
        let mut candidates = Vec::new();

        // 1. Detect Command (Action)
        if text_lower.contains("turn on") || text_lower.contains("turn off") || text_lower.starts_with("play") {
            candidates.push(IntentCandidate {
                id: Uuid::new_v4().to_string(),
                hypothesis: IntentHypothesis::Command,
                confidence: 0.9,
                source_symbol_ids: vec![symbol_id.to_string()],
                stability: IntentStability::Stable,
            });
        }
        // 2. Detect Inquiry
        else if text_lower.contains("what") || text_lower.contains("how") || text_lower.contains("?") {
            // Check for ambiguity
            if text_lower.contains("maybe") || text_lower.len() < 10 {
                candidates.push(IntentCandidate {
                    id: Uuid::new_v4().to_string(),
                    hypothesis: IntentHypothesis::Inquiry,
                    confidence: 0.6,
                    source_symbol_ids: vec![symbol_id.to_string()],
                    stability: IntentStability::Unstable, // Needs clarification
                });
            } else {
                candidates.push(IntentCandidate {
                    id: Uuid::new_v4().to_string(),
                    hypothesis: IntentHypothesis::Inquiry,
                    confidence: 0.85,
                    source_symbol_ids: vec![symbol_id.to_string()],
                    stability: IntentStability::Stable,
                });
            }
        }
        // 3. Detect Thinking Aloud / Fragment
        else if text_lower.contains("um") || text_lower.contains("uh") || text_lower.len() < 5 {
             candidates.push(IntentCandidate {
                id: Uuid::new_v4().to_string(),
                hypothesis: IntentHypothesis::ThinkingAloud,
                confidence: 0.7,
                source_symbol_ids: vec![symbol_id.to_string()],
                stability: IntentStability::Ambiguous,
            });
        }
        // 4. Default: Statement
        else {
             candidates.push(IntentCandidate {
                id: Uuid::new_v4().to_string(),
                hypothesis: IntentHypothesis::Statement,
                confidence: 0.5, // Low confidence by default
                source_symbol_ids: vec![symbol_id.to_string()],
                stability: IntentStability::Unstable,
            });
        }

        // Arbitration Logic
        // Find best candidate
        if let Some(best) = candidates.iter().max_by(|a, b| a.confidence.partial_cmp(&b.confidence).unwrap()) {
            match best.stability {
                IntentStability::Stable => IntentState::Stable(best.clone()),
                IntentStability::Unstable => IntentState::Forming(candidates),
                IntentStability::Ambiguous => IntentState::Forming(candidates),
            }
        } else {
            IntentState::None
        }
    }

    /// Decide the Dialogue Act based on the IntentState.
    /// Strict Rule: Phase G never executes (Wait/StaySilent).
    pub fn decide(&self, state: &IntentState) -> DialogueAct {
        match state {
            IntentState::None => DialogueAct::StaySilent,
            
            IntentState::Suspended(_) => DialogueAct::StaySilent, // Silence while suspended
            
            IntentState::Stable(_) => {
                // Handoff to Planner. Do NOT speak.
                DialogueAct::Wait 
            },
            
            IntentState::Forming(candidates) => {
                // Heuristic: If we have a high-ish confidence Unstable candidate, clarify.
                // If Ambiguous/ThinkingAloud, StaySilent.
                
                if let Some(best) = candidates.iter().max_by(|a, b| a.confidence.partial_cmp(&b.confidence).unwrap()) {
                    match best.hypothesis {
                        IntentHypothesis::ThinkingAloud => DialogueAct::StaySilent,
                        IntentHypothesis::Fragment => DialogueAct::StaySilent,
                        _ => {
                            if best.confidence > 0.5 && best.stability == IntentStability::Unstable {
                                // Rule: Non-leading clarification
                                DialogueAct::AskClarification("Do you want me to respond?".to_string())
                            } else {
                                DialogueAct::StaySilent
                            }
                        }
                    }
                } else {
                    DialogueAct::StaySilent
                }
            }
        }
    }
}
