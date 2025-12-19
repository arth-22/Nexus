#![cfg_attr(
  all(not(debug_assertions), target_os = "windows"),
  windows_subsystem = "windows"
)]

use tauri::Emitter;
// use nexus::kernel::presence::{PresenceRequest, PresenceState};

// Phase C: Commands
#[tauri::command]
fn send_input_fragment(text: String) {
    // UI -> Core Strict Boundary (Request Only)
    println!("[UI->Core] Input Fragment: '{}'", text);
    
    // In real impl: nexus::kernel::reactor::send(InputEvent::Fragment(text));
}

#[tauri::command]
fn toggle_mic(active: bool) {
    println!("[UI->Core] Mic Toggle: {}", active);
    // In real impl: nexus::kernel::audio::set_listening(active);
}

#[tauri::command]
fn ui_attach(app_handle: tauri::AppHandle) {
    println!("[UI->Core] UI Attached. Pushing Context...");
    
    // Push-Based Hydration (Mock for Phase C)
    // Core sends "ContextSnapshot" immediately on attach.
    // UI never asks "GetHistory".
    
    let mock_history = vec![
        serde_json::json!({"role": "user", "content": "Hello Nexus"}),
        serde_json::json!({"role": "system", "content": "Hello. I am listening."}),
    ];
    
    app_handle.emit("nexus-event", 
        serde_json::json!({
            "type": "ContextSnapshot",
            "content": mock_history
        })
    ).unwrap_or(());
}

fn main() {
    // 1. BOOTLOADER: Start Nexus Core (Detached)
    std::thread::spawn(|| {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            println!("[Core] Kernel starting in background...");
            loop {
                tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
                // println!("[Core] I am still here. Silence is safe.");
            }
        });
    });

    // 2. UI SHELL: Start Tauri
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            send_input_fragment, 
            toggle_mic,
            ui_attach
        ])
        .setup(|app| {
            let handle = app.handle().clone();
            // Mock Event Stream (Core -> UI) for Phase C Interaction Tests
            std::thread::spawn(move || {
                let mut tick = 0;
                loop {
                    std::thread::sleep(std::time::Duration::from_secs(5));
                    tick += 1;
                    
                    // Simulate occasional Presence Updates (No Animation Loop!)
                    if tick % 4 == 0 {
                        // Toggle Attentive / Engaged
                         handle.emit("nexus-event", 
                            serde_json::json!({
                                "type": "PresenceUpdate",
                                "state": if tick % 8 == 0 { "Engaged" } else { "Attentive" }
                            })
                        ).unwrap_or(());
                    }
                    
                    // Simulate occasional Output Draft -> Commit
                    if tick == 10 {
                         handle.emit("nexus-event", 
                            serde_json::json!({
                                "type": "OutputEvent",
                                "content": "Drafting...",
                                "status": "Draft"
                            })
                        ).unwrap_or(());
                    }
                    if tick == 12 {
                         handle.emit("nexus-event", 
                            serde_json::json!({
                                "type": "OutputEvent",
                                "content": "Output finalized.",
                                "status": "SoftCommit"
                            })
                        ).unwrap_or(());
                    }
                }
            });
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
