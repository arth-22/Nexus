use super::time::Tick;


#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct OutputId {
    pub tick: u64,
    pub ordinal: u16,
}

#[derive(Debug, Clone)]
pub enum Event {
    /// External signals (Audio, Text, System Signals)
    Input(InputEvent),
}

#[derive(Debug, Clone)]
pub struct InputEvent {
    pub source: String,
    // For Phase 0, we keep content simple. In real system, this is a struct.
    pub content: String, 
}

/// Intents are produced by the Planner (Pure)
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Intent {
    Log { msg: String },
    /// Request to emit an output
    Say { text: String },
    // Delay, Interrupt, etc. will go here
}

/// Outputs have a lifecycle managed by the Kernel
#[derive(Debug, Clone)]
pub struct Output {
    pub id: OutputId,
    pub content: String,
    pub status: OutputStatus,
    pub proposed_at: Tick,
    pub committed_at: Option<Tick>,
    pub parent_id: Option<String>, // UUID of parent task
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OutputStatus {
    Draft,
    Committed,
    Canceled,
}
