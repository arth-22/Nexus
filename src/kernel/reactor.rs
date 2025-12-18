use tokio::sync::mpsc;
use tokio::time::{interval, Duration}; // Only for the loop driver
use tracing::{info, warn};
use std::path::PathBuf;

use super::event::{Event, InputEvent};
use super::state::{SharedState, StateDelta};
use super::time::{Tick, TICK_MS};
use super::scheduler::{Scheduler, SideEffect};

use super::cancel::CancellationRegistry;
// use crate::planner::stub::plan;

use crate::planner::async_planner::AsyncPlanner;

// Memory System
use crate::memory::{
    MemoryObserver, MemoryConsolidator, InMemoryEpisodicStore, FileSemanticStore,
    EpisodicStore, SemanticStore // Traits
};
use crate::monitor::monitor::SelfObservationMonitor; // Monitor

pub struct Reactor {
    pub receiver: mpsc::Receiver<Event>,
    // We need a sender clone for the planner
    tx_clone: mpsc::Sender<Event>,
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
}

impl Reactor {
    pub fn new(receiver: mpsc::Receiver<Event>, tx: mpsc::Sender<Event>) -> Self {
        // Initialize Semantic Store
        // For now, store in the current directory or a known location.
        let semantic_path = PathBuf::from("nexus_semantic_memory.json");
        let mut semantic = FileSemanticStore::new(semantic_path);
        if let Err(e) = semantic.load() {
            warn!("Failed to load semantic memory: {:?}", e);
        }

        Self {
            receiver,
            tx_clone: tx.clone(),
            state: SharedState::new(),
            scheduler: Scheduler,
            cancel_registry: CancellationRegistry::new(),
            tick: Tick::new(),
            planner: AsyncPlanner::new(tx),
            last_planned_version: None,
            
            observer: MemoryObserver::new(),
            consolidator: MemoryConsolidator::new(),
            episodic: InMemoryEpisodicStore::new(),
            semantic,
            
            monitor: SelfObservationMonitor::new(),
        }
    }

    /// Pure Tick Step: Advances State. Returns SideEffects to be executed by the driver.
    /// MUST NOT await I/O or timers.
    /// 
    /// **KERNEL LAW**: The Tick is advanced at the VERY START of this step. 
    /// All reductions and planning occur in the context of the *new* tick.
    pub fn tick_step(&mut self, events: Vec<Event>) -> Vec<SideEffect> {
        self.tick = self.tick.next();
        self.state.reduce(StateDelta::Tick(self.tick)); // Sync Time
        let mut effects = Vec::new();

        // Separate inputs and plans
        let mut inputs = Vec::new();
        let mut plans = Vec::new();

        for event in events {
            match event {
                Event::Input(inp) => {
                     // 1. Vision: Check for PerceptUpdate -> Derive VisualState Delta
                     if let super::event::InputContent::Visual(super::event::VisualSignal::PerceptUpdate { hash, .. }) = inp.content {
                         // Stability logic
                         let current_stability = self.state.visual.stability_score;
                         let distance_val = match &inp.content {
                             super::event::InputContent::Visual(super::event::VisualSignal::PerceptUpdate { distance, .. }) => *distance,
                             _ => 0,
                         };
                         
                         let new_stability = if distance_val < 5 {
                             (current_stability + 0.1).min(1.0)
                         } else {
                             (current_stability - 0.3).max(0.0)
                         };
                         
                         // Emit derivation immediately
                         self.state.reduce(StateDelta::VisualStateUpdate {
                             hash,
                             stability: new_stability,
                         });
                         
                         // Add to inputs for CancelRegistry processing (which checks for Hard Interruptions)
                         inputs.push(inp); 
                     } else {
                         inputs.push(inp);
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

        // === 2. CANCEL (Pure Decision) ===
        let cancel_deltas = self.cancel_registry.process(&inputs);

        // === 3. REDUCE (Causality) ===
        for delta in cancel_deltas {
            self.state.reduce(delta);
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
                 super::event::InputContent::Audio(super::event::AudioSignal::SpeechStart) => {
                     // High Energy / Uncertainty
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
            if epoch.state_version == self.state.version || epoch.state_version + 1 == self.state.version {
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
                 let snapshot = self.state.snapshot(self.tick);
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
                         let ticks = (ms as u64) / crate::kernel::time::TICK_MS;
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
        
        let candidates = self.observer.flush();
        if !candidates.is_empty() {
             self.consolidator.process(
                 candidates, 
                 &mut self.episodic, 
                 &mut self.semantic, 
                 self.tick.frame
             );
        }
        
        // === SELF OBSERVATION MONITOR TICK ===
        // We feed aggregated observations collected earlier (monitor_obs) to the monitor
        if let Some(delta) = self.monitor.tick(self.tick.frame, &monitor_obs) {
             self.state.reduce(delta);
        }

        effects
    }

    /// Async Driver Loop
    pub async fn run(&mut self) {
        info!("Reactor Pipeline Started. Tick: {}ms", TICK_MS);

        let mut cadence = interval(Duration::from_millis(TICK_MS));
        cadence.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

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
                    SideEffect::Log(msg) => info!("[LOG] {}", msg),
                    SideEffect::SpawnAudio(id, text) => {
                        info!("[AUDIO-{:?}] Spawning: '{}'", id, text);
                    }
                }
            }
        }
    }
}
