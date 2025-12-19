#![cfg_attr(
  all(not(debug_assertions), target_os = "windows"),
  windows_subsystem = "windows"
)]

use tauri::{Manager, RunEvent};
// use nexus::kernel::presence::{PresenceRequest, PresenceState};
// In a real implementation we would import more from nexus::kernel

#[tauri::command]
fn send_input_signal(signal: String) {
    // UI -> Core Strict Boundary (Request Only)
    // This function acts as the "Input Boundary" described in Phase B
    println!("UI Sent Signal: {}", signal);
    
    // In a full implementation, this would send an async message to the Kernel Reactor.
    // For Phase B Prototype, we log it to prove the boundary exists.
    // nexus::kernel::reactor::send(InputEvent::from_signal(signal));
}

fn main() {
    // 1. BOOTLOADER: Start Nexus Core (Detached)
    // The Core owns its own runtime. UI death != Core death.
    std::thread::spawn(|| {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            println!("[Core] Kernel starting in background...");
            // nexus::start_kernel().await; 
            // Loop forever to prove detached survival
            loop {
                tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
                println!("[Core] I am still here. Silence is safe.");
            }
        });
    });

    // 2. UI SHELL: Start Tauri
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![send_input_signal])
        .setup(|app| {
            let handle = app.handle();
            // Mock Event Stream (Core -> UI) for Phase B Demo
            std::thread::spawn(move || {
                loop {
                    std::thread::sleep(std::time::Duration::from_secs(10));
                    // Simulate Core state change
                    handle.emit_all("nexus-event", 
                        serde_json::json!({
                            "type": "PresenceUpdate",
                            "state": "Attentive"
                        })
                    ).unwrap_or(());
                }
            });
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("error while running tauri application")
        .run(|_app_handle, event| match event {
            RunEvent::ExitRequested { api, .. } => {
                // PREVENT KERNEL SHUTDOWN
                // UI detach logic would go here.
                // For Phase B, we verify that closing the window does not kill the thread spawned above 
                // (except that main() ending kills threads unless we handle it).
                
                // Correction: In main(), if run() returns, the process exits.
                // To support "Core survives UI close", we must PREVENT exit if we want the process to stay alive
                // or ensure we run as a daemon. 
                // For Phase B "Desktop App", safe default is: App Close = Hide Window (on Mac) or truly Stop?
                // The requirement is "Core can exist without UI". 
                
                // Ideally we use: api.prevent_exit(); 
                // But for the Phase B Test "Core survives UI close", we might want to keep running.
                // Let's prevent exit to simulate background persistence.
                api.prevent_exit();
                println!("[Shell] Window closed. Core persists.");
            }
            _ => {}
        });
}
