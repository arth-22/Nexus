#![cfg_attr(
  all(not(debug_assertions), target_os = "windows"),
  windows_subsystem = "windows"
)]

use tauri::Emitter;
mod audio_capture;
use nexus::kernel::event::Event;

struct AudioState(audio_capture::AudioController);
struct CoreSender(tokio::sync::mpsc::Sender<Event>);

#[tauri::command]
fn send_input_fragment(text: String) {
    println!("[UI->Core] Input Fragment: '{}'", text);
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

fn main() {
    // 0. Init Logger
    tracing_subscriber::fmt::init();

    // 1. Setup Channels
    let (tx, rx) = tokio::sync::mpsc::channel(100);
    
    // 2. Setup Reactor (The Core)
    let mut reactor = nexus::kernel::reactor::Reactor::new(rx, tx.clone());
    
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

    tauri::Builder::default()
        .manage(AudioState(audio_controller))
        .manage(CoreSender(tx))
        .invoke_handler(tauri::generate_handler![
            send_input_fragment, 
            toggle_mic,
            ui_attach
        ])
        .setup(|app| {
            let _handle = app.handle().clone(); 
            
            // Spawn Kernel Thread
            std::thread::spawn(move || {
                println!("[Core] Kernel starting in background...");
                let rt = tokio::runtime::Runtime::new().unwrap();
                rt.block_on(async {
                    reactor.run().await;
                });
            });

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
