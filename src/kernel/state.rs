use super::event::{InputEvent, Output, OutputId, OutputStatus, InputContent, AudioSignal};
use super::presence::{PresenceState, PresenceRequest, PresenceGraph};
use std::collections::{HashMap, HashSet};
use crate::kernel::time::Tick;
use crate::kernel::intent::long_horizon::{LongHorizonIntent, IntentId};
use crate::kernel::audio::segment::{AudioSegment, SegmentStatus};
use crate::kernel::intent::types::IntentState;
use crate::kernel::memory::types::{MemoryCandidate, MemoryRecord, MemoryId, MemoryKey};
use crate::kernel::memory::consent::{MemoryConsent, MemoryConsentState};

#[derive(Debug, Clone)]
pub struct MetaLatents {
    /// 0.0 - 1.0: How sensitive the system is to being interrupted.
    /// Higher = System prefers shorter outputs or delays.
    pub interruption_sensitivity: f32,
    
    /// 0.0 - 1.0: Penalty on confidence due to recent failures.
    /// Higher = System requires higher internal confidence to gate output.
    pub confidence_penalty: f32,
    
    /// 0.0 - 1.0: Bias towards issuing correction intents.
    pub correction_bias: f32,
}

impl Default for MetaLatents {
    fn default() -> Self {
        Self {
            interruption_sensitivity: 0.0,
            confidence_penalty: 0.0,
            correction_bias: 0.0,
        }
    }
}

/// Strict state delta. This is the ONLY way state mutates.
#[derive(Debug, Clone)]
pub enum StateDelta {
    InputReceived(InputEvent),
    OutputProposed(Output),
    OutputCommitted(OutputId),
    OutputCanceled(OutputId),
    TaskCanceled(String),
    VisualStateUpdate { hash: u64, stability: f32 },
    LatentUpdate { slot: crate::kernel::latent::LatentSlot },
    MetaLatentUpdate { delta: MetaLatents }, 
    LongHorizonIntentUpdate(LongHorizonIntent),
    PresenceTransition(PresenceRequest),
    PresenceUpdate(PresenceState),
    // Audio Buffering Deltas
    AudioSegmentCreated(AudioSegment),
    AudioFrameAppended { segment_id: String, frames: Vec<f32> },
    AudioSegmentFinalized { segment_id: String, end_tick: Tick },
    AudioSegmentTranscribing(String),
    AudioSegmentTranscribed { segment_id: String, text: String },
    /// Phase G: Intent Assessment
    AssessmentUpdate(IntentState),
    Tick(Tick),
    // Phase H: Memory Consolidation
    MemoryCandidateCreated(MemoryCandidate),
    MemoryCandidateReinforced(MemoryId, Tick),
    MemoryPromoted(MemoryRecord),
    MemoryDecayed { id: MemoryId, new_strength: f32 },
    MemoryForgotten(MemoryId),
    MemoryCandidateRemoved(MemoryId), // Specific removal (e.g. after promotion)
    MemoryAccessed { id: MemoryId, time: Tick },
    // Phase L: Memory Consent
    MemoryConsentAsked(MemoryKey, Tick),
    MemoryConsentResolved { key: MemoryKey, state: MemoryConsentState, resolved_at: Tick },
}

#[derive(Debug, Clone)]
pub struct VisualState {
    pub hash: u64,
    pub stability_score: f32, // 0.0 - 1.0
}

impl Default for VisualState {
    fn default() -> Self {
        Self {
            hash: 0,
            stability_score: 0.0,
        }
    }
}

#[derive(Debug, Clone)]
pub struct SharedState {
    // Private fields to enforce encapsulation
    _beliefs: HashMap<String, f32>,
    active_outputs: HashMap<OutputId, Output>,
    // In strict model, we might track canceled task IDs or just effects
    canceled_tasks: HashSet<String>,
    // Monotonic version for Epoch validation
    pub version: u64,
    
    // Audio / Control State
    pub last_tick: Tick,
    pub user_speaking: bool,
    pub turn_pressure: f32, // 0.0 - 1.0
    pub last_speech_start: Option<Tick>,
    pub last_speech_end: Option<Tick>,
    pub hesitation_detected: bool,
    
    // Vision State
    pub visual: VisualState,
    
    // Latent Field (Sidecar)
    pub latents: crate::kernel::latent::LatentState,
    
    // Meta-Latents (Self-Observation)
    pub meta_latents: MetaLatents,
    
    // Long-Horizon Intents (Part IX)
    // Long-Horizon Intents (Part IX)
    pub active_intents: HashMap<IntentId, LongHorizonIntent>,

    // Phase B: Presence State (Authoritative)
    // Phase B: Presence State (Authoritative)
    pub presence: PresenceState,

    // Phase E: Audio Storage (Cognition)
    // Phase E: Audio Storage (Cognition)
    pub audio_segments: HashMap<String, AudioSegment>,
    pub active_segment_id: Option<String>,

    // Phase G: Intent Arbitration
    // Phase G: Intent Arbitration
    pub intent_state: IntentState,

    // Phase H: Memory Consolidation
    pub memory_candidates: HashMap<MemoryId, MemoryCandidate>,
    pub long_term_memory: HashMap<MemoryId, MemoryRecord>,
    // Phase L: Consent State (Human-Aligned)
    pub memory_consent: HashMap<MemoryKey, MemoryConsent>,
}

impl Default for SharedState {
    fn default() -> Self {
        Self {
            _beliefs: HashMap::new(),
            active_outputs: HashMap::new(),
            canceled_tasks: HashSet::new(),
            version: 0,
            last_tick: Tick { frame: 0 },
            user_speaking: false,
            turn_pressure: 0.0,
            last_speech_start: None,
            last_speech_end: None,
            hesitation_detected: false,
            visual: VisualState::default(), 
            latents: crate::kernel::latent::LatentState::default(),
            meta_latents: MetaLatents::default(),
            active_intents: HashMap::new(),
            presence: PresenceState::default(),
            audio_segments: HashMap::new(),
            active_segment_id: None,
            intent_state: IntentState::default(),
            memory_candidates: HashMap::new(),
            long_term_memory: HashMap::new(),
            memory_consent: HashMap::new(),
        }
    }
}

impl SharedState {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn snapshot(&self, tick: Tick, intent_context: crate::kernel::intent::long_horizon::IntentContext) -> crate::planner::types::StateSnapshot {
        crate::planner::types::StateSnapshot {
            epoch: crate::planner::types::PlanningEpoch {
                tick,
                state_version: self.version,
            },
            last_input_ticks: 0, // Placeholder
            user_active: self.user_speaking,
            active_outputs: self.active_outputs.len(),
            recent_interruptions: self.canceled_tasks.len(),
            latent_summary: {
                // Textual Firewall: Summarize slots to natural language
                let mut summary = String::new();
                for slot in &self.latents.slots {
                    use crate::kernel::latent::Modality;
                    let mod_str = match slot.modality {
                        Modality::Audio => "Audio",
                        Modality::Visual => "Visual",
                        Modality::Text => "Text",
                    };
                    // Only mention high confidence slots for now
                    if slot.confidence > 0.5 {
                        summary.push_str(&format!("{}: Conf {:.2}; ", mod_str, slot.confidence));
                    }
                }
                if summary.is_empty() { "Quiescent".to_string() } else { summary }
            },
            meta_mood: {
                let m = &self.meta_latents;
                let mut moods = Vec::new();
                if m.confidence_penalty > 0.3 { moods.push("Cautious"); }
                if m.interruption_sensitivity > 0.5 { moods.push("Sensitive"); }
                if m.correction_bias > 0.3 { moods.push("Reflective"); }
                if moods.is_empty() { "Confident".to_string() } else { moods.join(", ") }
            },
            intent_context,
        }
    }

    /// Pure reduction: State + Delta -> Mutated State
    pub fn reduce(&mut self, delta: StateDelta) {
        // Version increments on mutation (except maybe Tick?)
        // Let's increment on everything for safety.
        self.version += 1;
        
        match delta {
            StateDelta::Tick(t) => {
                self.last_tick = t;
                // Turn Pressure Dynamics
                // Decay if not speaking
                if !self.user_speaking {
                    self.turn_pressure = (self.turn_pressure - 0.01).max(0.0);
                } else {
                    // If speaking and system has active outputs (interruption)
                    if !self.active_outputs.is_empty() {
                         self.turn_pressure = (self.turn_pressure + 0.1).min(1.0);
                    }
                }
                
                // Visual Stability Decay (Physics)
                // If no update received this tick, decay slightly
                self.visual.stability_score = (self.visual.stability_score - 0.01).max(0.0);
                
                // Latent Decay (Physics)
                // Decay constant lambda ~ 0.1 for fast decay (Audio), 0.01 for slow (Vision)
                // confidence_new = confidence * exp(-rate) ~ confidence * (1.0 - rate)
                self.latents.slots.retain_mut(|slot| {
                    slot.confidence *= 1.0 - slot.decay_rate;
                    slot.confidence > 0.05 // Prune dead slots
                });
            }
            StateDelta::InputReceived(input) => {
                match input.content {
                    InputContent::Audio(AudioSignal::SpeechStart) => {
                        self.user_speaking = true;
                        self.last_speech_start = Some(self.last_tick);
                        self.hesitation_detected = false; 
                    }
                    InputContent::Audio(AudioSignal::SpeechEnd) => {
                        self.user_speaking = false;
                        self.last_speech_end = Some(self.last_tick);
                        
                        // Check Hesitation (Short burst < 10 ticks = 200ms)
                        if let Some(start) = self.last_speech_start {
                            // Tick should support subtraction or frame diff
                            if self.last_tick.frame >= start.frame {
                                let duration = self.last_tick.frame - start.frame;
                                if duration < 10 && duration > 0 {
                                    self.hesitation_detected = true;
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }
            StateDelta::OutputProposed(output) => {
                self.active_outputs.insert(output.id, output);
            }
            StateDelta::OutputCommitted(id) => {
                if let Some(out) = self.active_outputs.get_mut(&id) {
                    out.status = OutputStatus::Committed;
                }
            }
            StateDelta::OutputCanceled(id) => {
                if let Some(out) = self.active_outputs.get_mut(&id) {
                    out.status = OutputStatus::Canceled;
                }
            }
            StateDelta::TaskCanceled(task_id) => {
                self.canceled_tasks.insert(task_id.clone());
                // Cascade: Cancel all outputs belonging to this task
                for out in self.active_outputs.values_mut() {
                    if let Some(pid) = &out.parent_id {
                        if pid == &task_id {
                            out.status = OutputStatus::Canceled;
                        }
                    }
                }
            }
            StateDelta::VisualStateUpdate { hash, stability } => {
                self.visual.hash = hash;
                self.visual.stability_score = stability;
            }
            StateDelta::LatentUpdate { slot } => {
                self.latents.slots.push(slot);
            }
            StateDelta::MetaLatentUpdate { delta } => {
                // Replacement update (Monitor calculates new values)
                self.meta_latents = delta;
            }
            StateDelta::LongHorizonIntentUpdate(intent) => {
                self.active_intents.insert(intent.id.clone(), intent);
            }
            StateDelta::PresenceTransition(request) => {
                if let Some(new_state) = PresenceGraph::transition(self.presence, request) {
                    // TODO: Emit event? For now just mutate.
                    self.presence = new_state;
                }
                // If None, the transition was rejected by the Core (Authority).
            }
            StateDelta::PresenceUpdate(new_state) => {
                self.presence = new_state;
            }
            StateDelta::AudioSegmentCreated(seg) => {
                self.active_segment_id = Some(seg.id.clone());
                self.audio_segments.insert(seg.id.clone(), seg);
            }
            StateDelta::AudioFrameAppended { segment_id, frames } => {
                if let Some(seg) = self.audio_segments.get_mut(&segment_id) {
                    seg.frames.extend(frames);
                }
            }
            StateDelta::AudioSegmentFinalized { segment_id, end_tick } => {
                if let Some(seg) = self.audio_segments.get_mut(&segment_id) {
                    seg.end_tick = Some(end_tick);
                    seg.status = SegmentStatus::Pending;
                }
                if self.active_segment_id.as_ref() == Some(&segment_id) {
                    self.active_segment_id = None;
                }
            }
            StateDelta::AudioSegmentTranscribing(segment_id) => {
                if let Some(seg) = self.audio_segments.get_mut(&segment_id) {
                    seg.status = SegmentStatus::Transcribing;
                }
            }
            StateDelta::AudioSegmentTranscribed { segment_id, text } => {
                if let Some(seg) = self.audio_segments.get_mut(&segment_id) {
                    seg.status = SegmentStatus::Transcribed;
                    seg.transcription = Some(text);
                }
            }
            StateDelta::AssessmentUpdate(new_state) => {
                self.intent_state = new_state;
            }
            // Phase H: Memory Reduction
            StateDelta::MemoryCandidateCreated(candidate) => {
                self.memory_candidates.insert(candidate.id.clone(), candidate);
            }
            StateDelta::MemoryCandidateReinforced(id, tick) => {
                if let Some(candidate) = self.memory_candidates.get_mut(&id) {
                    candidate.reinforcement_count += 1;
                    candidate.last_reinforced_at = tick;
                }
            }
            StateDelta::MemoryPromoted(record) => {
                // Remove from candidates if promoted (Consolidator handles emitting Forgotten/Promoted pair, but good to ensure uniqueness)
                // We just insert into LTM here.
                self.long_term_memory.insert(record.id.clone(), record);
            }
            StateDelta::MemoryDecayed { id, new_strength } => {
                if let Some(record) = self.long_term_memory.get_mut(&id) {
                    record.strength = new_strength;
                }
            }
            StateDelta::MemoryForgotten(id) => {
                // Check both stores, though IDs should be unique / types distinct usually.
                // Assuming ID space is shared or we try both.
                self.memory_candidates.remove(&id);
                self.long_term_memory.remove(&id);
            }
            StateDelta::MemoryCandidateRemoved(id) => {
                self.memory_candidates.remove(&id);
            }

            StateDelta::MemoryAccessed { id, time } => {
                if let Some(record) = self.long_term_memory.get_mut(&id) {
                    record.last_accessed_at = time;
                }
            }
            StateDelta::MemoryConsentAsked(key, tick) => {
                let consent = MemoryConsent::new(key.clone(), tick);
                self.memory_consent.insert(key, consent);
            }
            StateDelta::MemoryConsentResolved { key, state, resolved_at } => {
                if let Some(consent) = self.memory_consent.get_mut(&key) {
                    consent.state = state;
                    consent.resolved_at = Some(resolved_at);
                } else {
                     // Should not happen, but if resolved without being strictly "asked" (maybe stale),
                     // we can insert it.
                     // But typically 'Asked' creates the unknown state first.
                     // If we resolve an unknown key, we insert it.
                     let mut consent = MemoryConsent::new(key.clone(), resolved_at);
                     consent.state = state;
                     consent.resolved_at = Some(resolved_at);
                     self.memory_consent.insert(key, consent);
                }
            }
        }
    }
    
    // Read-only accessors for Planner
    pub fn active_outputs(&self) -> &HashMap<OutputId, Output> {
        &self.active_outputs
    }

    pub fn canceled_tasks(&self) -> &std::collections::HashSet<String> {
        &self.canceled_tasks
    }
}
