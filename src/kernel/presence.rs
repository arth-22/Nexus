use serde::{Serialize, Deserialize};

/// The explicit lifecycle states of Nexus on a laptop.
/// Defined in Phase A (presence.md).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PresenceState {
    /// Process running, no listening, no visible UI, memory intact.
    Dormant,
    /// Listening enabled, no output, monitoring interruptions, waiting for stability.
    Attentive,
    /// Actively processing input, possibly drafting output, fully interruptible.
    /// NOTE: Engagement ≠ Responsiveness.
    Engaged,
    /// Long-horizon intent exists, no immediate action. 
    /// sustained intent across time, not immediate readiness to act.
    QuietlyHolding,
    /// User explicitly paused Nexus. Cognition frozen.
    Suspended,
}

impl Default for PresenceState {
    fn default() -> Self {
        Self::Dormant
    }
}

/// Commands that request a presence state transition.
/// These are REQUESTS, not forces. The State Machine validates them.
#[derive(Debug, Clone)]
pub enum PresenceRequest {
    SystemBoot,
    WakeWordDetected,
    InputActivity,
    OutputDrafted,
    OutputCompleted,
    LongTermIntentDetected,
    IntentResolved,
    UserSuspend,
    UserResume,
    Timeout,
}

/// The state machine that governs presence transitions.
/// Enforces the "Core Authority" and "Silence-Safe" rules.
pub struct PresenceGraph;

impl PresenceGraph {
    /// Pure function: (Current State, Request) -> New State
    /// Returns None if the transition is invalid/ignored.
    pub fn transition(current: PresenceState, request: PresenceRequest) -> Option<PresenceState> {
        use PresenceState::*;
        use PresenceRequest::*;

        match (current, request) {
            // --- From Dormant ---
            (Dormant, SystemBoot) => Some(Attentive), // or stays Dormant until UI attach? Let's say Attentive implies listening.
            // If boot implies "Ready to listen", then Attentive.
            
            // --- From Attentive ---
            (Attentive, WakeWordDetected) => Some(Engaged),
            (Attentive, InputActivity) => Some(Engaged), // Any typing/speech wakes it
            (Attentive, UserSuspend) => Some(Suspended),
            // Timeout in Attentive -> Dormant (Energy saving?) - Optional, let's keep it simple.

            // --- From Engaged ---
            (Engaged, OutputCompleted) => Some(Attentive), // Back to listening
            (Engaged, IntentResolved) => Some(Attentive),
            (Engaged, LongTermIntentDetected) => Some(QuietlyHolding),
            (Engaged, UserSuspend) => Some(Suspended),
            (Engaged, Timeout) => Some(Attentive), // If nothing happens, drift back

            // --- From QuietlyHolding ---
            (QuietlyHolding, InputActivity) => Some(Engaged), // Wakes up with context
            (QuietlyHolding, IntentResolved) => Some(Attentive), // Done holding
            (QuietlyHolding, UserSuspend) => Some(Suspended),

            // --- From Suspended ---
            (Suspended, UserResume) => Some(Attentive),

            // --- Loopbacks / No-Ops ---
            // Explicitly ignore invalid transitions to enforce authority
            _ => None,
        }
    }
}
