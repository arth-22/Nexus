use tokio::sync::mpsc;
use tokio::time::{interval, Duration}; // Only for the loop driver
use tracing::{info, warn};
use std::path::PathBuf;

use super::event::Event;
use super::state::{SharedState, StateDelta};
use super::time::{Tick, TICK_MS};
use super::scheduler::{Scheduler, SideEffect};

use super::cancel::CancellationRegistry;
// use crate::planner::stub::plan;

use crate::planner::async_planner::AsyncPlanner;
use uuid::Uuid;
use super::audio::segment::AudioSegment;

// Memory System
use crate::memory::{
    MemoryObserver, InMemoryEpisodicStore, FileSemanticStore,
    EpisodicStore, SemanticStore // Traits
};
use crate::kernel::memory::consolidator::MemoryConsolidator;
use crate::monitor::monitor::SelfObservationMonitor; // Monitor
use crate::kernel::intent::long_horizon::LongHorizonIntentManager;
use crate::kernel::telemetry::recorder::TelemetryRecorder;
use crate::kernel::telemetry::event::{TelemetryEvent, OutputEventKind, InterruptionSource};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KernelMode {
    Onboarding,
    Active,
}


#[derive(Debug, Clone, Copy)]
pub struct ReactorConfig {
    pub safe_mode: bool,
}

pub struct Reactor {
    pub receiver: mpsc::Receiver<Event>,
    // We need a sender clone for the planner
    _tx_clone: mpsc::Sender<Event>,
    pub state: SharedState,
    pub scheduler: Scheduler,
    pub cancel_registry: CancellationRegistry,
    pub tick: Tick,
    pub planner: AsyncPlanner,
    // Track the last state version we requested a plan for, to prevent loops
    last_planned_version: Option<u64>,

    // Memory Components (Sidecars)
    pub observer: MemoryObserver,
    pub consolidator: MemoryConsolidator,
    pub episodic: InMemoryEpisodicStore,
    pub semantic: FileSemanticStore,
    
    // Self-Observation Monitor
    pub monitor: SelfObservationMonitor,

    // Phase D: Audio Monitor (VAD)
    pub audio_monitor: crate::kernel::audio::monitor::AudioMonitor,
    
    // Part IX: Long-Horizon Intent Manager
    pub lhim: LongHorizonIntentManager,

    // Phase G: Intent Arbitrator
    pub arbitrator: crate::kernel::intent::arbitrator::IntentArbitrator,
    
    // Phase J: Telemetry
    pub telemetry: TelemetryRecorder,
    
    // Phase K: Onboarding Lock
    pub mode: KernelMode,

    // New config field
    pub config: ReactorConfig,
}

impl Reactor {
    pub fn new(receiver: mpsc::Receiver<Event>, sender: mpsc::Sender<Event>, config: ReactorConfig) -> Self {
        let _tx_clone = sender.clone(); 
        let mut telemetry = TelemetryRecorder::new();
        
        if config.safe_mode {
             telemetry.record(super::telemetry::event::TelemetryEvent::SafeModeActive);
        }

        // Initialize Semantic Store
        // For now, store in the current directory or a known location.
        let semantic_path = PathBuf::from("nexus_semantic_memory.json");
        let mut semantic = FileSemanticStore::new(semantic_path);
        if let Err(e) = semantic.load() {
            warn!("Failed to load semantic memory: {:?}", e);
        }

        Self {
            receiver,
            _tx_clone: sender.clone(), // Use the provided sender
            state: SharedState::new(),
            scheduler: Scheduler,
            cancel_registry: CancellationRegistry::new(),
            tick: Tick::new(),
            planner: AsyncPlanner::new(sender.clone()), // Use the provided sender
            last_planned_version: None,
            
            observer: MemoryObserver::new(),
            consolidator: MemoryConsolidator::new(),
            episodic: InMemoryEpisodicStore::new(),
            semantic,
            
            monitor: SelfObservationMonitor::new(),
            audio_monitor: crate::kernel::audio::monitor::AudioMonitor::new(48000),
            lhim: LongHorizonIntentManager::new(),
            arbitrator: crate::kernel::intent::arbitrator::IntentArbitrator::new(),
            telemetry, // Use the telemetry created above
            mode: KernelMode::Active, // Default to Active (Safe for Tests), Driver will override if needed.
            config, // Add the config field
        }
    }

    /// Set the kernel mode (Encapsulated)
    pub fn set_mode(&mut self, mode: KernelMode) {
        info!("Kernel Mode changed to: {:?}", mode);
        self.mode = mode;
    }

    /// Pure Tick Step: Advances State. Returns SideEffects to be executed by the driver.
    /// MUST NOT await I/O or timers.
    /// 
    /// **KERNEL LAW**: The Tick is advanced at the VERY START of this step. 
    /// All reductions and planning occur in the context of the *new* tick.
    pub fn tick_step(&mut self, events: Vec<Event>) -> Vec<SideEffect> {
        self.tick = self.tick.next();
        let _frame_start = self.tick.frame;
        let old_presence = self.state.presence; // Capture old presence for transition check
        
        self.state.reduce(StateDelta::Tick(self.tick)); // Sync Time
        let mut effects = Vec::new();

        // Separate inputs and plans
        let mut inputs = Vec::new();
        let mut plans = Vec::new();

        for event in events {
            match event {
                Event::Input(inp) => {
                     // Phase K Invariant: While in Onboarding, ALL user input is ignored.
                     // This is intentional and must not be relaxed.
                     if self.mode == KernelMode::Onboarding {
                         // We drop the input entirely.
                         // Do we log it? Maybe trace.
                         // tracing::trace!("Input dropped due to Onboarding Mode");
                         continue;
                     }

                     // 0. Pre-Process: Lifecycle Updates (AudioStatus)
                     if let super::event::InputContent::AudioStatus(ref status) = inp.content {
                          match status {
                               super::event::AudioStatus::PlaybackStarted => {
                                    self.audio_monitor.set_system_speaking(true);
                               }
                               super::event::AudioStatus::PlaybackEnded => {
                                    self.audio_monitor.set_system_speaking(false);
                               }
                          }
                     }

                     match &inp.content {
                         super::event::InputContent::AudioChunk(samples) => {
                             // Phase D: Core-side VAD
                             if let Some(signal) = self.audio_monitor.process(samples) {
                                  // Synthetic Event: VAD Signal
                                  let sig_evt = super::event::InputEvent {
                                      source: "CoreVAD".to_string(),
                                      content: super::event::InputContent::Audio(signal.clone())
                                  };
                                  inputs.push(sig_evt); 

                                  // Phase E: Buffer Cleanup on Signal
                                  match signal {
                                      super::event::AudioSignal::SpeechStart => {
                                          let new_id = Uuid::new_v4().to_string();
                                          let seg = AudioSegment::new(new_id, self.tick);
                                          self.state.reduce(StateDelta::AudioSegmentCreated(seg));
                                          
                                          // Phase G: Interruption Supremacy (Suspend Intent)
                                          if let crate::kernel::intent::types::IntentState::Forming(cands) = &self.state.intent_state {
                                              // Suspend the best candidate or just the set?
                                              // For MVP, if Forming, we suspend the *most confident* one to preserve context.
                                              if let Some(best) = cands.iter().max_by(|a, b| a.confidence.partial_cmp(&b.confidence).unwrap()) {
                                                  let susp = crate::kernel::intent::types::IntentState::Suspended(best.clone());
                                                  self.state.reduce(StateDelta::AssessmentUpdate(susp));
                                              }
                                          } else if let crate::kernel::intent::types::IntentState::Stable(cand) = &self.state.intent_state {
                                               let susp = crate::kernel::intent::types::IntentState::Suspended(cand.clone());
                                               self.state.reduce(StateDelta::AssessmentUpdate(susp));
                                          }
                                          // Note: If Suspended already, stay Suspended.
                                      }
                                      super::event::AudioSignal::SpeechEnd => {
                                          if let Some(id) = &self.state.active_segment_id {
                                              self.state.reduce(StateDelta::AudioSegmentFinalized { 
                                                  segment_id: id.clone(), 
                                                  end_tick: self.tick 
                                              });
                                          }
                                      }
                                  }
                             }
                             
                             // Phase E: Audio Buffering (Append Frame)
                             if let Some(id) = &self.state.active_segment_id {
                                 let id_clone = id.clone(); // Clone ID to avoid borrow issues
                                 self.state.reduce(StateDelta::AudioFrameAppended { 
                                     segment_id: id_clone, 
                                     frames: samples.clone() 
                                 });
                             }
                         },
                         // IMPORTANT: Handle explicit Audio signals (e.g. from Tests or External VAD)
                         super::event::InputContent::Audio(ref signal) => {
                             match signal {
                                 super::event::AudioSignal::SpeechStart => {
                                      if self.state.active_segment_id.is_none() {
                                          let new_id = Uuid::new_v4().to_string();
                                          let seg = AudioSegment::new(new_id, self.tick);
                                          self.state.reduce(StateDelta::AudioSegmentCreated(seg));
                                          
                                          // Phase G: Interruption Supremacy (Suspend Intent)
                                          if let crate::kernel::intent::types::IntentState::Forming(cands) = &self.state.intent_state {
                                              if let Some(best) = cands.iter().max_by(|a, b| a.confidence.partial_cmp(&b.confidence).unwrap()) {
                                                  let susp = crate::kernel::intent::types::IntentState::Suspended(best.clone());
                                                  self.state.reduce(StateDelta::AssessmentUpdate(susp));
                                              }
                                          } else if let crate::kernel::intent::types::IntentState::Stable(cand) = &self.state.intent_state {
                                               let susp = crate::kernel::intent::types::IntentState::Suspended(cand.clone());
                                               self.state.reduce(StateDelta::AssessmentUpdate(susp));
                                          }
                                      }
                                 }
                                 super::event::AudioSignal::SpeechEnd => {
                                      if let Some(id) = &self.state.active_segment_id {
                                          self.state.reduce(StateDelta::AudioSegmentFinalized { 
                                              segment_id: id.clone(), 
                                              end_tick: self.tick 
                                          });
                                      }
                                 }
                             }
                             inputs.push(inp);
                         },
                         super::event::InputContent::Visual(super::event::VisualSignal::PerceptUpdate { hash, distance }) => {
                             // Stability logic
                             let current_stability = self.state.visual.stability_score;
                             let distance_val = *distance;
                             
                             let new_stability = if distance_val < 5 {
                                 (current_stability + 0.1).min(1.0)
                             } else {
                                 (current_stability - 0.3).max(0.0)
                             };
                             
                             // Emit derivation immediately
                             self.state.reduce(StateDelta::VisualStateUpdate {
                                 hash: *hash,
                                 stability: new_stability,
                             });
                             
                             // Add to inputs for CancelRegistry processing
                             inputs.push(inp); 
                         },
                         super::event::InputContent::TranscriptionRequest { segment_id } => {
                             // Gating Logic: Check availability
                             let mut accepted = false;
                             if let Some(seg) = self.state.audio_segments.get(segment_id) {
                                 if seg.status == crate::kernel::audio::segment::SegmentStatus::Pending {
                                     accepted = true;
                                 }
                             }
                             
                             if accepted {
                                 self.state.reduce(StateDelta::AudioSegmentTranscribing(segment_id.clone()));
                                 effects.push(SideEffect::RequestTranscription { segment_id: segment_id.clone() });
                             } else {
                                 warn!("Transcription Request DENIED for segment: {}", segment_id);
                             }
                         },
                          super::event::InputContent::ProvisionalText { content, confidence: _, source_id } => {
                              self.state.reduce(StateDelta::AudioSegmentTranscribed { 
                                  segment_id: source_id.clone(), 
                                  text: content.clone() 
                              });
                              
                              // Phase G: Assess & Decide
                              let new_intent_state = self.arbitrator.assess(content, source_id, &self.state.intent_state);
                              self.state.reduce(StateDelta::AssessmentUpdate(new_intent_state.clone()));
                              
                              // Phase H: Memory Ingest (Edge Triggered)
                              if let crate::kernel::intent::types::IntentState::Stable(cand) = &new_intent_state {
                                  // Memory
                                  let memory_deltas = self.consolidator.process_intent(cand, &self.state, &mut self.telemetry);
                                  for d in memory_deltas {
                                      self.state.reduce(d);
                                  }
                                  
                                  // Phase I: Long-Horizon Intent Registration
                                  // This is the primary entry point for Intent Creation
                                  let intent_deltas = self.lhim.register_intent(cand, &self.state, self.tick, &mut self.telemetry);
                                  for d in intent_deltas {
                                      self.state.reduce(d);
                                  }
                              }

                              // Decide
                              let dialogue_act = self.arbitrator.decide(&self.state.intent_state); 
                              // (Using state.intent_state which is now updated)
                              
                              match dialogue_act {
                                  crate::kernel::intent::types::DialogueAct::AskClarification(msg) => {
                                      // Emit Audio Side Effect
                                      // We need an OutputId. For Phase G, we can mock or create a robust one.
                                      let out_id = crate::kernel::event::OutputId { tick: self.tick.frame, ordinal: 99 };
                                      effects.push(SideEffect::SpawnAudio(out_id, msg));
                                  }
                                  crate::kernel::intent::types::DialogueAct::Confirm(msg) => {
                                      let out_id = crate::kernel::event::OutputId { tick: self.tick.frame, ordinal: 99 };
                                      effects.push(SideEffect::SpawnAudio(out_id, msg));
                                  }
                                  crate::kernel::intent::types::DialogueAct::Offer(msg) => {
                                      let out_id = crate::kernel::event::OutputId { tick: self.tick.frame, ordinal: 99 };
                                      effects.push(SideEffect::SpawnAudio(out_id, msg));
                                  }
                                  crate::kernel::intent::types::DialogueAct::Wait => {
                                      // Hand off to planner (do nothing here, let planner see Stable state)
                                  }
                                  crate::kernel::intent::types::DialogueAct::StaySilent => {
                                      // Do nothing
                                  }
                              }

                              inputs.push(inp); // Propagate text to other systems
                          },

                         super::event::InputContent::MemoryConsentResponse { key, state } => {
                             self.state.reduce(StateDelta::MemoryConsentResolved {
                                 key: key.clone(),
                                 state: state.clone(),
                                 resolved_at: self.tick,
                             });
                             // Telemetry
                             self.telemetry.record(TelemetryEvent::MemoryEvent {
                                 kind: crate::kernel::telemetry::event::MemoryEventKind::AttributesUpdated, // Or new ConsentResolved kind? Use AttributesUpdated for now.
                                 memory_id: "consent_update".to_string(), // Metadata
                             });
                         },
                         _ => {
                             inputs.push(inp);
                         }
                     }
                },
                Event::PlanProposed(epoch, intent) => plans.push((epoch, intent)),
            }
        }
        
        // === MONITOR OBSERVATION (RAW INPUTS) ===
        // Feed raw inputs to Monitor
        let mut monitor_obs = Vec::new();
        for inp in &inputs {
            let user_obs = self.monitor.observe_raw(inp, &self.state);
            monitor_obs.extend(user_obs);
        }

        // TELEMETRY: Check Presence Transition
        if self.state.presence != old_presence {
            self.telemetry.record(TelemetryEvent::PresenceTransition {
                from: old_presence,
                to: self.state.presence,
                tick: self.tick,
            });
        }
        
        // TELEMETRY: Silence Tracking
        // Definition: No user speech, no system speech.
        // We check state flags.
        // AudioMonitor tracks system_speaking.
        // SharedState tracks user_speaking.
        if !self.state.user_speaking && !self.audio_monitor.is_system_speaking() {
             self.telemetry.record(TelemetryEvent::SilencePeriod { duration_ticks: 1 });
        }

        // === 2. CANCEL (Pure Decision) ===
        let cancel_deltas = self.cancel_registry.process(&inputs);
        let has_cancellation = !cancel_deltas.is_empty();

        if has_cancellation {
            effects.push(SideEffect::StopAudio);
        }

        // === 3. REDUCE (Causality) ===
        for delta in cancel_deltas {
            // TELEMETRY: Output Cancellation
            if let StateDelta::OutputCanceled(id) = &delta {
                // Find proposed output to measure latency?
                // Or just record event.
                // We record explicit cancel event.
                self.telemetry.record(TelemetryEvent::OutputLifecycle {
                    output_id: *id,
                    event: OutputEventKind::Cancelled,
                    latency_ticks: 0, // Instantaneous
                });
            }
            self.state.reduce(delta);
        }
        
        // TELEMETRY: Interruption
        if has_cancellation {
            // Latency = 0 (Same tick processing)
             self.telemetry.record(TelemetryEvent::Interruption {
                source: InterruptionSource::ExplicitCancel, // Default/Inferred
                cancel_latency_ticks: 0,
            });
        }
        
        // === PART IX: LONG-HORIZON INTENT (INTERRUPTION SUPREMACY & LIFECYCLE) ===
        
        // 1. Interruption Supremacy (Suspend Active Intents)
        // Triggered by: Cancellation OR SpeechStart detected this tick.
        let mut interruption_detected = has_cancellation;
        
        // Check input inputs for SpeechStart
        if !interruption_detected {
             for inp in &inputs {
                 if let crate::kernel::event::InputContent::Audio(crate::kernel::event::AudioSignal::SpeechStart) = &inp.content {
                     interruption_detected = true;
                     break;
                 }
             }
        }

        if interruption_detected {
            let intent_deltas = self.lhim.handle_interruption(&self.state, self.tick, &mut self.telemetry);
            for d in intent_deltas {
                self.state.reduce(d);
            }
        }

        // 2. Apply Decay (Time-based monoticity)
        let decay_deltas = self.lhim.tick(self.tick, &self.state, &mut self.telemetry);
        for d in decay_deltas {
            self.state.reduce(d);
        }

        // 3. Attempt Resumption (Silent Context Match)
        // Only if we are NOT currently interrupted/inputting
        if inputs.is_empty() && !interruption_detected {
             let resume_deltas = self.lhim.try_resume(&self.state, self.tick, &mut self.telemetry);
             for d in resume_deltas {
                 self.state.reduce(d);
             }
        }
        
        if !inputs.is_empty() {
             // CRITICAL: Input invalidates current planning context. 
             // Stop the thinker.
             self.planner.abort();
             // We reset planned version because we interrupted the thought process
             // although the state version mismatch will handle it naturally.
        }

        for inp in inputs.iter() {
            self.state.reduce(StateDelta::InputReceived(inp.clone()));
        }
        
        // === 3.5 DERIVE LATENTS (Passive Sidecar) ===
        // Must happen AFTER structural reduction (Scoping Rule).
        // Bias, do not Override.
        
        for inp in inputs.iter() {
             match &inp.content {
                 crate::kernel::event::InputContent::Audio(crate::kernel::event::AudioSignal::SpeechStart) => {
                     // High Energy / Uncertainty -> Presence Request
                     if let Some(new_state) = crate::kernel::presence::PresenceGraph::transition(
                         self.state.presence, 
                         crate::kernel::presence::PresenceRequest::AudioActivity
                     ) {
                         self.state.reduce(StateDelta::PresenceUpdate(new_state));
                     }

                     self.state.reduce(StateDelta::LatentUpdate {
                         slot: crate::kernel::latent::LatentSlot {
                             values: vec![1.0], // Simplified "Energy" vector
                             confidence: 0.8,
                             created_at: self.tick,
                             modality: crate::kernel::latent::Modality::Audio,
                             decay_rate: 0.1, // Fast decay
                         }
                     });
                 }
                 super::event::InputContent::Visual(super::event::VisualSignal::PerceptUpdate { .. }) => {
                     // Concept: Vision is an Anchor. 
                     // Low Uncertainty if stable.
                     // We just record the presence of visual ground.
                      self.state.reduce(StateDelta::LatentUpdate {
                         slot: crate::kernel::latent::LatentSlot {
                             values: vec![0.5], // "Stability" vector
                             confidence: 0.8,
                             created_at: self.tick,
                             modality: crate::kernel::latent::Modality::Visual,
                             decay_rate: 0.01, // Slow decay (Persistence)
                         }
                     });
                 }
                 _ => {}
             }
        }

        // === MEMORY OBSERVATION (LATENTS) ===
        // Observe current latent state for candidates
        for slot in &self.state.latents.slots {
            self.observer.observe_latent(slot, self.tick.frame);
        }

        // === 4. PLAN (Async Integration) ===
        // A) Apply VALID Proposed Plans
        let mut intents = Vec::new();
        for (epoch, intent) in plans {
            // STALE REJECTION
            // Allow version 0 for manual/debug injections
            if epoch.state_version == 0 || epoch.state_version == self.state.version || epoch.state_version + 1 == self.state.version {
                 intents.push(intent);
            } else {
                info!("Discarded Stale Plan: Epoch {:?} vs State {}", epoch, self.state.version);
            }
        }

        // B) Check Opportunity -> Speculate
        // If state is quiescent, ask LLM.
        // GUARD: Only plan if we haven't already planned for this state version
        if self.state.active_outputs().is_empty() {
             let needs_plan = match self.last_planned_version {
                 Some(v) => v != self.state.version,
                 None => true,
             };

             if needs_plan {
                 let context = self.lhim.get_context(&self.state);
                 let snapshot = self.state.snapshot(self.tick, context);
                 // Future: Inject Memory Retrieval into Snapshot here?
                 // Or does planner query it via tool?
                 // Plan says: "Planner Query -> Memory Retriever".
                 // So we don't inject passively yet.
                 self.planner.dispatch(snapshot);
                 self.last_planned_version = Some(self.state.version);
             }
        }
        
        // === 5. EMIT & 6. SCHEDULE === 
        for (ordinal, intent) in intents.into_iter().enumerate() {
            // PHASE 6: Crystallization Gate
            // Intercept BeginResponse
            if let crate::planner::types::Intent::BeginResponse { .. } = &intent {
                 use crate::kernel::crystallizer::{check_gate, extract_snapshot, CrystallizationDecision};
                 use crate::outputs::realizer::realize;
                 
                 let decision = check_gate(&self.state);
                 match decision {
                     CrystallizationDecision::Deny => {
                         info!("Gate DENIED response crystallization due to instability.");
                         continue;
                     }
                     CrystallizationDecision::Delay { ms } => {
                         info!("Gate DELAYED response by {}ms.", ms);
                         let ticks = (ms as u64) / crate::kernel::time::TICK_MS + 1; // Round up
                         // +1 to ensure at least 1 tick
                         let (delta_opt, effect_opt) = self.scheduler.schedule(
                             crate::planner::types::Intent::Delay { ticks }, 
                             self.tick, 
                             ordinal as u16
                         );
                         if let Some(delta) = delta_opt { self.state.reduce(delta); }
                         if let Some(effect) = effect_opt { effects.push(effect); }
                         continue;
                     }
                     CrystallizationDecision::AllowPartial | CrystallizationDecision::AllowHard => {
                         // Realize Text
                         let snapshot = extract_snapshot(&self.state);
                         let text = realize(&snapshot, &decision);
                         let status = match decision {
                             CrystallizationDecision::AllowHard => crate::kernel::event::OutputStatus::HardCommit,
                             _ => crate::kernel::event::OutputStatus::SoftCommit,
                         };
                         
                         // Create Output
                         let output_id = crate::kernel::event::OutputId { 
                             tick: self.tick.frame, 
                             ordinal: ordinal as u16 
                         };
                         
                         let output_obj = crate::kernel::event::Output {
                             id: output_id,
                             content: text.clone(),
                             status, 
                             proposed_at: self.tick,
                             committed_at: None,
                             parent_id: None,
                         };

                          let delta = StateDelta::OutputProposed(output_obj.clone());
                          self.state.reduce(delta);
                          
                          // TELEMETRY: Output Draft Started (Hard Commit really, since we just crystallized)
                          // Wait, OutputProposed -> DraftStarted? 
                          // Or HardCommit? The status is in output_obj.
                          let kind = match output_obj.status {
                              crate::kernel::event::OutputStatus::HardCommit => OutputEventKind::HardCommit,
                              _ => OutputEventKind::SoftCommit,
                          };
                          
                          self.telemetry.record(TelemetryEvent::OutputLifecycle {
                              output_id,
                              event: kind,
                              latency_ticks: 0, 
                          });
                         
                         let effect = SideEffect::SpawnAudio(output_id, text.clone()); 
                         effects.push(effect);

                         // === MEMORY OBSERVATION (OUTPUT) ===
                         self.observer.observe_crystallization(&output_obj, &snapshot, self.tick.frame);
                         
                         continue;
                     }
                 }
            }
        
            let (delta_opt, effect_opt) = self.scheduler.schedule(intent, self.tick, ordinal as u16);
            if let Some(delta) = delta_opt { self.state.reduce(delta); }
            if let Some(effect) = effect_opt { effects.push(effect); }
        }

        // === MEMORY CONSOLIDATION ===
        // Drive Memory Lifecycle
        self.episodic.tick(self.tick.frame); // Decay
        
        // let candidates = self.observer.flush();
        // if !candidates.is_empty() {
        //      // Old Consolidator Process - Deprecated by Phase H Kernel Memory
        //      /*
        //      self.consolidator.process(
        //          candidates, 
        //          &mut self.episodic, 
        //          &mut self.semantic, 
        //          self.tick.frame
        //      );
        //      */
        // }
        
        // === SELF OBSERVATION MONITOR TICK ===
        // We feed aggregated observations collected earlier (monitor_obs) to the monitor
        if let Some(delta) = self.monitor.tick(self.tick.frame, &monitor_obs) {
             self.state.reduce(delta);
        }

        // === PHASE H: MEMORY TICK ===
        // SAFE MODE CHECK: Block memory consolidation
        let mem_tick_deltas = if self.config.safe_mode {
            Vec::new() // No memory logic in safe mode
        } else {
            self.consolidator.tick(self.tick, &self.state, &mut self.telemetry)
        };

        for d in mem_tick_deltas {
            if let StateDelta::MemoryConsentAsked(key, _) = &d {
                 // Double check safe mode (redundant but safe)
                 if self.config.safe_mode { continue; }

                 effects.push(SideEffect::AskMemoryConsent { 
                     key: key.clone(), 
                     prompt_id: Uuid::new_v4().to_string() 
                 });
            }
            self.state.reduce(d);
        }

        effects
    }

    /// Async Driver Loop
    pub async fn run(&mut self) {
        info!("Reactor Pipeline Started. Tick: {}ms", TICK_MS);

        let mut cadence = interval(Duration::from_millis(TICK_MS));
        cadence.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

        let mut audio_child: Option<tokio::sync::oneshot::Sender<()>> = None;

        loop {
            // Driver: Wait for physical time boundary
            cadence.tick().await;

            // Driver: Drain Events (Inputs + Plans)
            let mut events: Vec<Event> = Vec::new();
            while let Ok(event) = self.receiver.try_recv() {
                events.push(event);
            }

            // Core: Execute Step
            let effects = self.tick_step(events);

            // Driver: Execute Side Effects
            for effect in effects {
                match effect {
                    SideEffect::Log(msg) => println!("[LOG] {}", msg),
                    SideEffect::SpawnAudio(id, text) => {
                        println!("[AUDIO-{:?}] Spawning 'say': '{}'", id, text);
                        // [Temporary Phase D Output Harness]
                        // 1. Kill existing
                        if let Some(stop_tx) = audio_child.take() {
                             let _ = stop_tx.send(()); 
                        }
                        // 2. Spawn new (macOS only for Phase D)
                        // Use "say" command
                        match tokio::process::Command::new("say")
                            .arg(&text)
                            .kill_on_drop(true) // Ensure it dies if we drop handle
                            .spawn() 
                        {
                            Ok(mut child) => {
                                let tx_clone = self._tx_clone.clone();
                                let (stop_tx, mut stop_rx) = tokio::sync::oneshot::channel();
                                
                                tokio::spawn(async move {
                                    // Signal Started
                                    let _ = tx_clone.send(Event::Input(crate::kernel::event::InputEvent {
                                        source: "Driver".to_string(),
                                        content: crate::kernel::event::InputContent::AudioStatus(
                                            crate::kernel::event::AudioStatus::PlaybackStarted
                                        )
                                    })).await;

                                    // Race: Completion vs Kill
                                    tokio::select! {
                                        _ = child.wait() => { 
                                            // Natural Finish or Error
                                        }
                                        _ = &mut stop_rx => {
                                            // Kill Signal
                                            let _ = child.kill().await;
                                        }
                                    }
                                    
                                    // Signal Ended (Normalized)
                                    let _ = tx_clone.send(Event::Input(crate::kernel::event::InputEvent {
                                        source: "Driver".to_string(),
                                        content: crate::kernel::event::InputContent::AudioStatus(
                                            crate::kernel::event::AudioStatus::PlaybackEnded
                                        )
                                    })).await;
                                });
                                
                                audio_child = Some(stop_tx);
                            },
                            Err(e) => warn!("Failed to spawn audio: {}", e),
                        }
                    },
                    SideEffect::StopAudio => {
                         if let Some(stop_tx) = audio_child.take() {
                            println!("[AUDIO] KILL SWITCH ACTIVATED.");
                            let _ = stop_tx.send(());
                        }
                    },
                    SideEffect::RequestTranscription { segment_id } => {
                        info!("[TRANSCRIPTION] Requested for Segment: {}", segment_id);
                        
                        // 1. Retrieve Audio from SharedState
                        let audio_data_opt = self.state.audio_segments.get(&segment_id).map(|seg| seg.frames.clone());
                        let tx = self._tx_clone.clone();

                        if let Some(frames) = audio_data_opt {
                            tokio::spawn(async move {
                                // 2. Write to WAV (Temp)
                                let file_path = format!("/tmp/nexus_seg_{}.wav", segment_id);
                                let spec = hound::WavSpec {
                                    channels: 1,
                                    sample_rate: 48000,
                                    bits_per_sample: 32,
                                    sample_format: hound::SampleFormat::Float,
                                };
                                
                                if let Ok(mut writer) = hound::WavWriter::create(&file_path, spec) {
                                    for &sample in &frames {
                                        writer.write_sample(sample).unwrap();
                                    }
                                    writer.finalize().unwrap();
                                    info!("[TRANSCRIPTION] Saved WAV to {}", file_path);

                                    // 3. Spawn ASR (Mocked for now with delay, or verify whisper exists)
                                    // For this phase, we act as a "Mock ASR" that returns text after delay.
                                    tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
                                    
                                    let mock_text = "Phase E verification successful. Gate is working.";
                                    
                                    // Send result back
                                    let _ = tx.send(Event::Input(crate::kernel::event::InputEvent {
                                        source: "ASR".to_string(),
                                        content: crate::kernel::event::InputContent::ProvisionalText {
                                            content: mock_text.to_string(),
                                            confidence: 0.9,
                                            source_id: segment_id.clone(),
                                        }
                                    })).await;
                                } else {
                                    tracing::error!("[TRANSCRIPTION] Failed to write WAV file");
                                }
                            });
                        } else {
                            warn!("[TRANSCRIPTION] Segment not found in state: {}", segment_id);
                        }
                    }

                    SideEffect::AskMemoryConsent { key, prompt_id: _ } => {
                        // In Reactor test driver, we just log it. 
                        // Real driver handles it in main.rs
                        println!("[REACTOR-LOG] Ask Consent for key: {:?}", key);
                    }
                }
            }
        }
    }
}
