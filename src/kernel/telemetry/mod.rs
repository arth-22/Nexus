//! Phase J: Instrumentation & Cognitive Telemetry
//! 
//! # SAFETY INVARIANT
//! Telemetry is a READ-ONLY side-effect layer. 
//! It must **NEVER** be read inside decision logic (Kernel, Planner, or Arbitrator).
//! It exists solely for observability and verification.
//!
//! # PRIVACY INVARIANT
//! Telemetry events must **NEVER** contain user content (Text, Audio, Embeddings).
//! Only internal IDs (IntentId, MemoryId, OutputId) and metrics (Duration, Counts) are allowed.

pub mod event;
pub mod metrics;
pub mod recorder;
