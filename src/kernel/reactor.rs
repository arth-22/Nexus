use tokio::sync::mpsc;
use tokio::time::{interval, Duration}; // Only for the loop driver
use tracing::info;

use super::event::{Event, InputEvent};
use super::state::{SharedState, StateDelta};
use super::time::{Tick, TICK_MS};
use super::scheduler::{Scheduler, SideEffect};

use super::cancel::CancellationRegistry;
use crate::planner::stub::plan;

pub struct Reactor {
    pub receiver: mpsc::Receiver<Event>,
    pub state: SharedState,
    pub scheduler: Scheduler,
    pub cancel_registry: CancellationRegistry,
    pub tick: Tick,
}

impl Reactor {
    pub fn new(receiver: mpsc::Receiver<Event>) -> Self {
        Self {
            receiver,
            state: SharedState::new(),
            scheduler: Scheduler,
            cancel_registry: CancellationRegistry::new(),
            tick: Tick::new(),
        }
    }

    /// Pure Tick Step: Advances State. Returns SideEffects to be executed by the driver.
    /// MUST NOT await I/O or timers.
    pub fn tick_step(&mut self, inputs: Vec<InputEvent>) -> Vec<SideEffect> {
        self.tick = self.tick.next();
        let mut effects = Vec::new();

        // === 2. CANCEL (Pure Decision) ===
        let cancel_deltas = self.cancel_registry.process(&inputs);

        // === 3. REDUCE (Causality) ===
        for delta in cancel_deltas {
            self.state.reduce(delta);
        }
        for inp in inputs {
            self.state.reduce(StateDelta::InputReceived(inp));
        }

        // === 4. PLAN (Pure Futures) ===
        let intents = plan(&self.state, self.tick);
        
        // === 5. EMIT (Speculation) & 6. SCHEDULE ===
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

            // Driver: Drain Inputs
            let mut inputs: Vec<InputEvent> = Vec::new();
            while let Ok(event) = self.receiver.try_recv() {
                if let Event::Input(inp) = event {
                    inputs.push(inp);
                }
            }

            // Core: Execute Step
            let effects = self.tick_step(inputs);

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
