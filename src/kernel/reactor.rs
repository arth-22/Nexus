use tokio::sync::mpsc;
use tokio::time::{interval, Duration}; // Only for the loop driver
use tracing::info;

use super::event::{Event, InputEvent};
use super::state::{SharedState, StateDelta};
use super::time::{Tick, TICK_MS};
use super::scheduler::{Scheduler, SideEffect};

use super::cancel::CancellationRegistry;
// use crate::planner::stub::plan;

use crate::planner::async_planner::AsyncPlanner;

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
}

impl Reactor {
    pub fn new(receiver: mpsc::Receiver<Event>, tx: mpsc::Sender<Event>) -> Self {
        Self {
            receiver,
            tx_clone: tx.clone(),
            state: SharedState::new(),
            scheduler: Scheduler,
            cancel_registry: CancellationRegistry::new(),
            tick: Tick::new(),
            planner: AsyncPlanner::new(tx),
            last_planned_version: None,
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
                Event::Input(inp) => inputs.push(inp),
                Event::PlanProposed(epoch, intent) => plans.push((epoch, intent)),
            }
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

        for inp in inputs {
            self.state.reduce(StateDelta::InputReceived(inp));
        }

        // === 4. PLAN (Async Integration) ===
        // A) Apply VALID Proposed Plans
        let mut intents = Vec::new();
        for (epoch, intent) in plans {
            // STALE REJECTION: Only accept if epoch matches current state version
            // Note: In real system, we might allow slightly older versions if casual.
            // For Strict Phase 1: Epoch.state_version must match self.state.version
            // Actually, state.version might have incremented due to inputs in *this* tick.
            // So we allow epoch.version <= self.version? 
            // Better: We check if the State hasn't diverged significantly.
            // For now: Strict Equality is safest (but brittle). Let's accept if epoch.state_version is "recent".
            // But user guide said: "check RoundId".
            // Let's enforce strictness: If state changed, your plan is invalid.
            if epoch.state_version == self.state.version {
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
                 self.planner.dispatch(snapshot);
                 self.last_planned_version = Some(self.state.version);
             }
        }
        
        // === 5. EMIT & 6. SCHEDULE === 
        for (ordinal, intent) in intents.into_iter().enumerate() {
            let (delta_opt, effect_opt) = self.scheduler.schedule(intent, self.tick, ordinal as u16);
            
            if let Some(delta) = delta_opt {
                self.state.reduce(delta);
            }
            
            if let Some(effect) = effect_opt {
                effects.push(effect);
            }
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
