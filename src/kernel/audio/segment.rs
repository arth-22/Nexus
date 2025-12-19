use super::super::time::Tick;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum SegmentStatus {
    Buffering,
    Pending,     // Audio captured, waiting for Intent/Decision
    Transcribing,
    Transcribed, // Has text
    Discarded,   // Purged or ignored
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AudioSegment {
    pub id: String,
    pub frames: Vec<f32>,
    pub start_tick: Tick,
    pub end_tick: Option<Tick>,
    pub status: SegmentStatus,
    // Optional: Metadata for transcription
    pub transcription: Option<String>,
}

impl AudioSegment {
    pub fn new(id: String, start_tick: Tick) -> Self {
        Self {
            id,
            frames: Vec::new(),
            start_tick,
            end_tick: None,
            status: SegmentStatus::Buffering,
            transcription: None,
        }
    }
}
