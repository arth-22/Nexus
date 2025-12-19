use super::types::MemoryKey;
use crate::kernel::time::Tick;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MemoryConsentState {
    Unknown,
    Granted,
    Declined,
    Ignored,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryConsent {
    pub memory_key: MemoryKey,
    pub state: MemoryConsentState,
    pub asked_at: Tick,
    pub resolved_at: Option<Tick>,
}

impl MemoryConsent {
    pub fn new(key: MemoryKey, asked_at: Tick) -> Self {
        Self {
            memory_key: key,
            state: MemoryConsentState::Unknown,
            asked_at,
            resolved_at: None,
        }
    }
}
