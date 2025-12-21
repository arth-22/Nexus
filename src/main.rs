use nexus::kernel::reactor::Reactor;
use tokio::sync::mpsc;
use uuid::Uuid;
use nexus::kernel::event::Event;
use nexus::kernel::scheduler::SideEffect;
use std::collections::HashMap;
use tokio::task::JoinHandle;
use std::time::{Duration, Instant};

// Internal Driver Events (Never touch Kernel)
enum DriverEvent {
    GeneratedSpeech { output_id: Uuid, text: String },
    SpeechFailed { output_id: Uuid },
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging/tracing
    tracing_subscriber::fmt::init();
    tracing::info!("Nexus Kernel Booting...");

    // Kernel Channel
    let (tx, rx) = mpsc::channel(100);

    // Driver Internal Channel
    let (driver_tx, mut driver_rx) = mpsc::channel(100);

    // Setup Reactor
    let config = nexus::kernel::reactor::ReactorConfig { safe_mode: false };
    let mut reactor = Reactor::new(rx, tx.clone(), config);

    // Initialize Services
    let llm_service = nexus::services::llm::client::LLMService::new();
    
    // Driver State
    let mut speech_tasks: HashMap<Uuid, JoinHandle<()>> = HashMap::new();
    let mut speech_dedupe: HashMap<Uuid, Instant> = HashMap::new();
    let mut audio_child: Option<tokio::sync::oneshot::Sender<()>> = None;

    // Clone tx for audio status reporting check
    let status_tx = tx.clone();
    
    let mut cadence = tokio::time::interval(Duration::from_millis(100));
    cadence.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

    tracing::info!("Nexus Kernel Active. Press Ctrl+C to stop.");

    loop {
         cadence.tick().await;

         // 1. Drain Kernel Events
         let mut events = Vec::new();
         while let Ok(event) = reactor.receiver.try_recv() {
             events.push(event);
         }

         // 2. Drain Driver Events (Async Results)
         while let Ok(evt) = driver_rx.try_recv() {
             match evt {
                 DriverEvent::GeneratedSpeech { output_id, text } => {
                     // Check if task was cancelled/removed?
                     if speech_tasks.contains_key(&output_id) {
                         speech_tasks.remove(&output_id); // Completed
                         
                         // Telemetry: Generated
                         let _ = status_tx.send(Event::Telemetry(
                             nexus::kernel::telemetry::event::TelemetryEvent::SpeechLifecycle(
                                 nexus::kernel::telemetry::event::SpeechLifecycleEvent::Generated
                             )
                         )).await;

                         // PLAY AUDIO (The "Harness" Logic)
                         println!("[AUDIO-{:?}] Spawning 'say': '{}'", output_id, text);
                         if let Some(stop_tx) = audio_child.take() {
                             let _ = stop_tx.send(());
                         }
                         
                         match tokio::process::Command::new("say")
                             .arg(&text)
                             .kill_on_drop(true)
                             .spawn() 
                         {
                             Ok(mut child) => {
                                 let tx_clone = status_tx.clone();
                                 let (stop_tx, mut stop_rx) = tokio::sync::oneshot::channel();
                                 audio_child = Some(stop_tx);
                                 
                                 tokio::spawn(async move {
                                     let _ = tx_clone.send(Event::Input(nexus::kernel::event::InputEvent {
                                         source: "Driver".to_string(),
                                         content: nexus::kernel::event::InputContent::AudioStatus(
                                             nexus::kernel::event::AudioStatus::PlaybackStarted
                                         )
                                     })).await;

                                     tokio::select! {
                                         _ = child.wait() => {}
                                         _ = &mut stop_rx => { let _ = child.kill().await; }
                                     }
                                     
                                     let _ = tx_clone.send(Event::Input(nexus::kernel::event::InputEvent {
                                         source: "Driver".to_string(),
                                         content: nexus::kernel::event::InputContent::AudioStatus(
                                             nexus::kernel::event::AudioStatus::PlaybackEnded
                                         )
                                     })).await;
                                 });
                             }
                             Err(e) => tracing::warn!("Failed to spawn 'say': {}", e),
                         }
                     }
                 },
                 DriverEvent::SpeechFailed { output_id } => {
                     speech_tasks.remove(&output_id);
                     let _ = status_tx.send(Event::Telemetry(
                         nexus::kernel::telemetry::event::TelemetryEvent::SpeechLifecycle(
                             nexus::kernel::telemetry::event::SpeechLifecycleEvent::Failed
                         )
                     )).await;
                 }
             }
         }

         // 3. Kernel Step
         let effects = reactor.tick_step(events);

         // 4. Handle Side Effects
         for effect in effects {
             match effect {
                 SideEffect::Log(msg) => println!("[LOG] {}", msg),
                 
                 SideEffect::SpawnAudio(_output_id_legacy, text) => {
                     // Legacy Harness (direct spawn)
                     // Re-use logic or duplicate? Duplicate for minimal friction now.
                     // Mock ID for tracking audio handle
                     println!("[AUDIO-LEGACY] Spawning 'say': '{}'", text);
                     if let Some(stop_tx) = audio_child.take() { let _ = stop_tx.send(()); }
                     
                     match tokio::process::Command::new("say").arg(&text).kill_on_drop(true).spawn() {
                         Ok(mut child) => {
                             let tx_clone = status_tx.clone();
                             let (stop_tx, mut stop_rx) = tokio::sync::oneshot::channel();
                             audio_child = Some(stop_tx);
                             tokio::spawn(async move {
                                 let _ = tx_clone.send(Event::Input(nexus::kernel::event::InputEvent {
                                     source: "Driver".to_string(),
                                     content: nexus::kernel::event::InputContent::AudioStatus(nexus::kernel::event::AudioStatus::PlaybackStarted)
                                 })).await;
                                 tokio::select! { _ = child.wait() => {}, _ = &mut stop_rx => { let _ = child.kill().await; } }
                             });
                         }
                         Err(_) => {}
                     }
                 },
                 
                 SideEffect::StopAudio => {
                     if let Some(stop_tx) = audio_child.take() { let _ = stop_tx.send(()); }
                     for (_, task) in speech_tasks.drain() { task.abort(); }
                     let _ = status_tx.send(Event::Telemetry(
                         nexus::kernel::telemetry::event::TelemetryEvent::SpeechLifecycle(
                             nexus::kernel::telemetry::event::SpeechLifecycleEvent::Aborted
                         )
                     )).await;
                 },

                 SideEffect::RequestSpeech { intent, output_id } => {
                     // Dedupe
                     if let Some(inst) = speech_dedupe.get(&output_id) {
                         if inst.elapsed() < Duration::from_secs(10) { continue; }
                     }
                     speech_dedupe.insert(output_id, Instant::now());
                     
                     // Telemetry: Requested
                     let _ = status_tx.send(Event::Telemetry(
                         nexus::kernel::telemetry::event::TelemetryEvent::SpeechLifecycle(
                             nexus::kernel::telemetry::event::SpeechLifecycleEvent::Requested
                         )
                     )).await;

                     // Spawn Task
                     let service = llm_service.clone();
                     let dr_tx = driver_tx.clone();
                     let oid = output_id;
                     
                     let task = tokio::spawn(async move {
                         // Hard Timeout 2s
                         let result = tokio::time::timeout(Duration::from_secs(2), service.generate_speech(intent)).await;
                         
                         match result {
                             Ok(Ok(text)) => {
                                 let _ = dr_tx.send(DriverEvent::GeneratedSpeech { output_id: oid, text }).await;
                             },
                             Ok(Err(e)) => {
                                 tracing::warn!("LLM Error: {}", e);
                                 let _ = dr_tx.send(DriverEvent::SpeechFailed { output_id: oid }).await;
                             },
                             Err(_) => { // Timeout
                                 tracing::warn!("LLM Timeout");
                                 let _ = dr_tx.send(DriverEvent::SpeechFailed { output_id: oid }).await;
                             }
                         }
                     });
                     
                     speech_tasks.insert(output_id, task);
                 },
                 
                 _ => {}
             }
         }
         
         // Cleanup Dedupe (TTL)
         speech_dedupe.retain(|_, time| time.elapsed() < Duration::from_secs(10));
    }
}
