use crate::kernel::intent::types::DialogueAct;

#[derive(Debug, Clone, PartialEq)]
pub enum SpeechIntent {
    Clarification(String), // Seed logic, e.g. "intent_ambiguous"
    Confirmation(String),
    Offer(String),
}

pub struct SpeechPlanner;

impl SpeechPlanner {
    pub fn new() -> Self {
        Self
    }

    pub fn plan(&self, act: &DialogueAct, safe_mode: bool) -> Option<SpeechIntent> {
        if safe_mode {
            return None;
        }

        match act {
            DialogueAct::AskClarification(reason) => {
                Some(SpeechIntent::Clarification(reason.clone()))
            },
            DialogueAct::Confirm(details) => {
                Some(SpeechIntent::Confirmation(details.clone()))
            },
            DialogueAct::Offer(details) => {
                Some(SpeechIntent::Offer(details.clone()))
            },
            DialogueAct::Wait | DialogueAct::StaySilent => None,
        }
    }
}
