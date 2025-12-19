#![cfg_attr(
  all(not(debug_assertions), target_os = "windows"),
  windows_subsystem = "windows"
)]

use tauri::Emitter;
use tauri::Manager;
mod audio_capture;
use nexus::kernel::event::Event;
use nexus::kernel::reactor::KernelMode;
use std::sync::{Arc, Mutex};
use std::path::PathBuf;
use std::fs;
use serde::{Serialize, Deserialize};
mod alpha;
use alpha::AlphaAccess;

struct AudioState(audio_capture::AudioController);
struct CoreSender(tokio::sync::mpsc::Sender<Event>);
struct ReactorHandle(Arc<Mutex<nexus::kernel::reactor::Reactor>>);

#[derive(Serialize, Deserialize, Default)]
struct OnboardingState {
    completed: bool,
    completed_at: Option<u64>,
    #[serde(default)]
    welcome_shown: bool,
}

fn onboarding_file_path(app: &tauri::AppHandle) -> PathBuf {
    let config_dir = app.path().app_config_dir().expect("Failed to get config dir");
    fs::create_dir_all(&config_dir).ok();
    config_dir.join("onboarding.json")
}

fn load_onboarding_state(app: &tauri::AppHandle) -> OnboardingState {
    let path = onboarding_file_path(app);
    if path.exists() {
        if let Ok(content) = fs::read_to_string(&path) {
            if let Ok(state) = serde_json::from_str(&content) {
                return state;
            }
        }
    }
    OnboardingState::default()
}

fn save_onboarding_state(app: &tauri::AppHandle, state: &OnboardingState) {
    let path = onboarding_file_path(app);
    if let Ok(content) = serde_json::to_string_pretty(state) {
        fs::write(path, content).ok();
    }
}

#[tauri::command]
fn send_input_fragment(text: String) {
    println!("[UI->Core] Input Fragment: '{}'", text);
}

#[tauri::command]
fn get_onboarding_status(app: tauri::AppHandle) -> bool {
    let state = load_onboarding_state(&app);
    state.completed
}

// --- Phase M: Welcome Logic ---
#[tauri::command]
fn should_show_welcome(app: tauri::AppHandle) -> bool {
    let state = load_onboarding_state(&app);
    !state.welcome_shown
}

#[tauri::command]
fn mark_welcome_seen(app: tauri::AppHandle) {
    let mut state = load_onboarding_state(&app);
    state.welcome_shown = true;
    save_onboarding_state(&app, &state);
    println!("[Welcome] Marked as seen.");
}

#[tauri::command]
fn complete_onboarding(app: tauri::AppHandle, reactor_handle: tauri::State<ReactorHandle>) {
    // 1. Persist
    let state = OnboardingState {
        completed: true,
        completed_at: Some(std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs()),
        welcome_shown: false, // Explicitly false so they see it next
    };
    save_onboarding_state(&app, &state);
    println!("[Onboarding] State persisted.");

    // 2. Unlock Kernel
    if let Ok(mut reactor) = reactor_handle.0.lock() {
        reactor.set_mode(KernelMode::Active);
        // 3. Emit Telemetry (Lifecycle)
        reactor.telemetry.record(nexus::kernel::telemetry::event::TelemetryEvent::Lifecycle(
            nexus::kernel::telemetry::event::LifecycleEvent::OnboardingCompleted
        ));
        println!("[Onboarding] Kernel unlocked and Telemetry emitted.");
    }
}

#[tauri::command]
fn toggle_mic(active: bool, state: tauri::State<AudioState>, app: tauri::AppHandle) {
    // Controller is thread safe (holds Sender)
    if active {
        println!("[Command] Mic ON");
        state.0.start();
        // Feedback: Tell UI we are Listening (since Core presence logic isn't wired to UI stream yet)
        app.emit("nexus-event", 
            serde_json::json!({
                "type": "PresenceUpdate",
                "state": "Attentive" // Listening
            })
        ).unwrap_or(());
    } else {
        println!("[Command] Mic OFF");
        state.0.stop();
        app.emit("nexus-event", 
            serde_json::json!({
                "type": "PresenceUpdate",
                "state": "Engaged" // Active but not listening? Or Dormant?
            })
        ).unwrap_or(());
    }
}

#[tauri::command]
async fn ui_attach(app_handle: tauri::AppHandle, core_state: tauri::State<'_, CoreSender>) -> Result<(), ()> {
    // Phase M: Check Access
    // Note: We already check access in setup(), but this check protects late-binding UI.
    if let Some(access) = AlphaAccess::load(&app_handle) { 
       // Logic to check specific UI permissions if needed
    }
    
    // We'll trust the Setup hook to handle the "not spawning" part.
    // Here we just tell the UI what's up.
    let access = crate::alpha::AlphaAccess::load(&app_handle);
    if access.is_none() || !access.unwrap().enabled {
        app_handle.emit("access-denied", ()).unwrap_or(());
        return Ok(());
    }

    println!("[UI->Core] UI Attached. Pushing Context...");
    
    // 1. Send Mock Context to UI
    let mock_history = vec![
        serde_json::json!({"role": "user", "content": "Hello Nexus"}),
        serde_json::json!({"role": "system", "content": "System Ready. Toggle Mic to test Interruption."}),
    ];
    
    app_handle.emit("nexus-event", 
        serde_json::json!(
            {
                "type": "ContextSnapshot",
                "content": mock_history
            }
        )
    ).unwrap_or(());

    // 2. Inject Verification Intent: "Hello Phase D"
    // We construct a PlanProposed event manually
    let intent = nexus::planner::types::Intent::BeginResponse { confidence: 1.0 };
    let tick = nexus::kernel::time::Tick { frame: 0 };
    let epoch = nexus::planner::types::PlanningEpoch { tick, state_version: 0 }; // Mock epoch
    
    let evt = Event::PlanProposed(epoch, intent);
    
    println!("[Verification] Injecting BeginResponse Intent...");
    if let Err(e) = core_state.0.send(evt).await {
        println!("[Error] Failed to inject intent: {}", e);
    }

    Ok(())
}

#[tauri::command]
fn resolve_memory_consent(key_json: String, state: String, core_state: tauri::State<'_, CoreSender>) {
    // Deserialize Key
    if let Ok(key) = serde_json::from_str::<nexus::kernel::memory::types::MemoryKey>(&key_json) {
        let consent_state = match state.as_str() {
            "granted" => nexus::kernel::memory::consent::MemoryConsentState::Granted,
            "declined" => nexus::kernel::memory::consent::MemoryConsentState::Declined,
            _ => nexus::kernel::memory::consent::MemoryConsentState::Ignored,
        };
        
        let evt = Event::Input(nexus::kernel::event::InputEvent {
            source: "Frontend".to_string(),
            content: nexus::kernel::event::InputContent::MemoryConsentResponse {
                key,
                state: consent_state,
            }
        });
        
        let _ = core_state.0.try_send(evt);
    } else {
        println!("[Error] Failed to deserialize MemoryKey in resolve_memory_consent");
    }
}

fn main() {
    // 0. Init Logger
    tracing_subscriber::fmt::init();

    // 1. Setup Channels
    let (tx, rx) = tokio::sync::mpsc::channel(100);
    
    // Parses CLI args to check for safe-mode
    // Note: Tauri's arg parsing happens inside run context usually, but we need it for Kernel init
    // We'll rely on env var "NEXUS_SAFE_MODE=1" or simple arg scan for this alpha phase
    let safe_mode = std::env::args().any(|arg| arg == "--safe-mode") || std::env::var("NEXUS_SAFE_MODE").is_ok();

    if safe_mode {
        println!("[Main] Safe Mode Detected. Core memory disabled.");
    }

    // 2. Setup Reactor (The Core)
    let config = nexus::kernel::reactor::ReactorConfig { safe_mode };
    let reactor = nexus::kernel::reactor::Reactor::new(rx, tx.clone(), config);
    let reactor_arc = Arc::new(Mutex::new(reactor));
    
    // 3. Audio Actor (Shell -> AudioThread -> Core)
    let (cmd_tx, cmd_rx) = tokio::sync::mpsc::channel(10);
    let audio_core_tx = tx.clone();
    
    // Spawn Audio Actor Thread
    println!("[Main] Spawning Audio Thread...");
    std::thread::spawn(move || {
        println!("[AudioThread] Running closure...");
        let actor = audio_capture::AudioActor::new(cmd_rx, audio_core_tx);
        actor.run();
    });

    let audio_controller = audio_capture::AudioController::new(cmd_tx);

    // Clone for Tauri state
    let reactor_handle = ReactorHandle(reactor_arc.clone());

    tauri::Builder::default()
        .manage(AudioState(audio_controller))
        .manage(CoreSender(tx.clone()))
        .manage(reactor_handle)
        .invoke_handler(tauri::generate_handler![
            send_input_fragment, 
            toggle_mic,
            ui_attach,
            get_onboarding_status,
            complete_onboarding,
            resolve_memory_consent,
            should_show_welcome,
            mark_welcome_seen
        ])

    .setup(move |app| {
        let handle = app.handle().clone();

        // --- Phase M: Strict Access Gate ---
        let alpha_access = AlphaAccess::load(&handle);
        
        let kernel_allowed = if let Some(access) = &alpha_access {
            access.enabled
        } else {
            false
        };

        if !kernel_allowed {
            // SILENT DENIAL - Do not spawn audio or kernel.
            // Just emit denial event for frontend to show static screen.
            // We use a small delay to ensure Frontend works, or we wait for UI attach.
            // We will do it on UI attach via event.
            // But we must NOT spawn threads.
            println!("[AccessBarrier] Alpha access missing or disabled. Kernel suppressed.");
            // We exit this closure without spawning threads.
            
            // Set up a listener for UI attach to tell it Access Denied?
            // Actually, we can just *not* do anything.
            // But main.rs UI attach command expects CoreSender state.
            // If we don't spawn threads, we might panic on missing state usage?
            // Tauri setup: we already managed CoreSender globally.
            // But the RX end is in the reactor.
            // The TX is managed.
            
            // This is "Inert Mode".
            return Ok(());
        }

        // --- Access Granted: Proceed to Boot ---
        
        // Phase K: Check onboarding and set initial kernel mode
            let onboarding_state = load_onboarding_state(&handle);
            {
                if let Ok(mut reactor) = reactor_arc.lock() {
                    if onboarding_state.completed {
                        reactor.set_mode(KernelMode::Active);
                        println!("[Onboarding] Already completed. Kernel Active.");
                    } else {
                        reactor.set_mode(KernelMode::Onboarding);
                        println!("[Onboarding] Not completed. Kernel locked.");
                    }
                }
            }
            
            // Clone Arc for the kernel thread
            let reactor_for_thread = reactor_arc.clone();
            // Clone Arc for the kernel thread
            let reactor_for_thread = reactor_arc.clone();
            let kernel_tx = tx.clone();
            let handle_for_thread = handle.clone();
            
            // Spawn Kernel Thread
            std::thread::spawn(move || {
                println!("[Core] Kernel starting in background...");
                let rt = tokio::runtime::Runtime::new().unwrap();
                rt.block_on(async {
                    // We need to run the reactor loop, but reactor is behind a Mutex.
                    // The run() method takes &mut self.
                    // This is a problem with Arc<Mutex>.
                    // Solution: Change Reactor::run() to take Arc<Mutex>, or
                    // Extract run logic here.
                    // For MVP, we'll call tick_step manually in a loop.
                    use tokio::time::{interval, Duration};
                    use tokio::process::Command; // Ensure Command is available

                    let mut cadence = interval(Duration::from_millis(nexus::kernel::time::TICK_MS));
                    let mut audio_child: Option<tokio::sync::oneshot::Sender<()>> = None;

                    cadence.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
                    
                    loop {
                        cadence.tick().await;
                        
                        // Drain events and tick
                        let mut effects = Vec::new();
                        {
                            if let Ok(mut reactor) = reactor_for_thread.lock() {
                                // Drain
                                let mut events = Vec::new();
                                while let Ok(event) = reactor.receiver.try_recv() {
                                    events.push(event);
                                }
                                effects = reactor.tick_step(events);
                            }
                        }
                        
                        // Execute side effects OUTSIDE lock
                        // (Complex side effects like SpawnAudio need async context)
                        // For now, just log them. Full effect handling is complex.
                        for effect in effects {
                            match effect {
                                nexus::kernel::scheduler::SideEffect::Log(msg) => println!("[LOG] {}", msg),
                                nexus::kernel::scheduler::SideEffect::SpawnAudio(id, text) => {
                                    println!("[AUDIO-{:?}] Spawning 'say': '{}'", id, text);
                                    
                                    // 1. Kill existing
                                    if let Some(stop_tx) = audio_child.take() {
                                         let _ = stop_tx.send(()); 
                                    }

                                    // 2. Spawn new (macOS only)
                                    match Command::new("say")
                                        .arg(&text)
                                        .kill_on_drop(true)
                                        .spawn() 
                                    {
                                        Ok(mut child) => {
                                            let tx_clone = kernel_tx.clone();
                                            let (stop_tx, mut stop_rx) = tokio::sync::oneshot::channel();
                                            
                                            tokio::spawn(async move {
                                                // Signal Started
                                                let _ = tx_clone.send(Event::Input(nexus::kernel::event::InputEvent {
                                                    source: "Driver".to_string(),
                                                    content: nexus::kernel::event::InputContent::AudioStatus(
                                                        nexus::kernel::event::AudioStatus::PlaybackStarted
                                                    )
                                                })).await;

                                                // Race: Completion vs Kill
                                                tokio::select! {
                                                    _ = child.wait() => {}
                                                    _ = &mut stop_rx => {
                                                        let _ = child.kill().await;
                                                    }
                                                }
                                                
                                                // Signal Ended
                                                let _ = tx_clone.send(Event::Input(nexus::kernel::event::InputEvent {
                                                    source: "Driver".to_string(),
                                                    content: nexus::kernel::event::InputContent::AudioStatus(
                                                        nexus::kernel::event::AudioStatus::PlaybackEnded
                                                    )
                                                })).await;
                                            });
                                            
                                            audio_child = Some(stop_tx);
                                        },
                                        Err(e) => println!("[AUDIO] Failed to spawn: {}", e),
                                    }
                                },
                                nexus::kernel::scheduler::SideEffect::StopAudio => {
                                    if let Some(stop_tx) = audio_child.take() {
                                        println!("[AUDIO] KILL SWITCH ACTIVATED.");
                                        let _ = stop_tx.send(());
                                    }
                                },
                                nexus::kernel::scheduler::SideEffect::RequestTranscription { segment_id } => {
                                    println!("[TRANSCRIPTION] Requested for: {}", segment_id);
                                }
                                nexus::kernel::scheduler::SideEffect::AskMemoryConsent { key, prompt_id: _ } => {
                                    println!("[CONSENT] Asking user for key: {:?}", key);
                                    let _ = handle_for_thread.emit("ask-memory-consent", serde_json::json!({
                                        "key": key
                                    }));
                                }
                            }
                        }
                    }
                });
            });

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
