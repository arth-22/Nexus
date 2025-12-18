use serde::{Deserialize, Serialize};

/// Objective observations about system behavior and user reaction.
/// Does NOT include subjective judgments like "OverExplanation".
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum SelfObservation {
    /// System output was interrupted by the user quickly (e.g. within 500ms).
    UnexpectedInterruption { output_id: Option<String> },

    /// User explicitly corrected the system (e.g. "No", "Wrong").
    UserCorrection { output_id: Option<String> },

    /// User stopped the output early but didn't correct it (e.g. "Okay enough").
    ResponseTruncation { output_id: Option<String> },

    /// System had high confidence but the plan failed/was canceled.
    ConfidenceMismatch { expected: f32, actual_outcome: String },

    /// System completed intent chain successfully without interruption.
    StableAlignment,
    
    /// User confirmed the statement (e.g. "Yes", "Exactly").
    Confirmation,
}

#[derive(Debug, Clone)]
pub struct MetaObservationEvent {
    pub observation: SelfObservation,
    pub confidence: f32, // How sure the monitor is that this happened
    pub timestamp: u64,
}
