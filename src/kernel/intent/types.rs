use serde::{Serialize, Deserialize};

pub type IntentId = String;
pub type SymbolId = String; // Maps to AudioSegmentID

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum IntentStability {
    Stable,
    Unstable,
    Ambiguous,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum IntentHypothesis {
    Inquiry,       // User asking something
    Statement,     // User stating something
    Command,       // User instructing Action
    Fragment,      // Incomplete thought
    ThinkingAloud, // Self-talk
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct IntentCandidate {
    pub id: IntentId,
    pub hypothesis: IntentHypothesis,
    pub confidence: f32, // 0.0 to 1.0
    pub source_symbol_ids: Vec<SymbolId>, // Symbolic Grounding
    pub stability: IntentStability,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum IntentState {
    None,
    Forming(Vec<IntentCandidate>),
    /// Ready for Handoff. Phase G never executes this.
    Stable(IntentCandidate),
    /// Interrupted but preserved. No output until reinforced.
    Suspended(IntentCandidate),
}

impl Default for IntentState {
    fn default() -> Self {
        Self::None
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum DialogueAct {
    /// Request clarification. MUST be non-leading (e.g. "Do you want me to respond?").
    AskClarification(String),
    /// Confirm understanding before acting (optional).
    Confirm(String),
    /// Offer an action path (e.g. "I can explain X or Y").
    Offer(String),
    /// Active Wait: We have an intent (Stable) but are handling it off (to Planner).
    Wait,
    /// Passive Silence: No sufficient intent to speak.
    StaySilent,
}
