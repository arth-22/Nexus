use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use tokio::sync::mpsc;
use nexus::kernel::event::{Event, InputEvent, InputContent};
use tracing::{info, error};

pub enum AudioCommand {
    Start,
    Stop,
}

pub struct AudioController {
    cmd_tx: mpsc::Sender<AudioCommand>,
}

impl AudioController {
    pub fn new(cmd_tx: mpsc::Sender<AudioCommand>) -> Self {
        Self { cmd_tx }
    }

    pub fn start(&self) {
        let _ = self.cmd_tx.blocking_send(AudioCommand::Start);
    }

    pub fn stop(&self) {
        let _ = self.cmd_tx.blocking_send(AudioCommand::Stop);
    }
}

pub struct AudioActor {
    stream: Option<cpal::Stream>,
    core_tx: mpsc::Sender<Event>,
    cmd_rx: mpsc::Receiver<AudioCommand>,
}

impl AudioActor {
    pub fn new(cmd_rx: mpsc::Receiver<AudioCommand>, core_tx: mpsc::Sender<Event>) -> Self {
        Self {
            stream: None,
            core_tx,
            cmd_rx,
        }
    }

    pub fn run(mut self) {
        println!("[AudioActor] Thread Alive.");
        println!("[AudioActor] Started.");
        
        while let Some(cmd) = self.cmd_rx.blocking_recv() {
            match cmd {
                AudioCommand::Start => {
                    if self.stream.is_none() {
                        match self.create_stream() {
                            Ok(s) => {
                                println!("[Audio] Stream Created & Started.");
                                self.stream = Some(s);
                            },
                            Err(e) => println!("[Audio] Failed to start stream: {}", e),
                        }
                    } else {
                        println!("[Audio] Stream already running.");
                    }
                },
                AudioCommand::Stop => {
                    if self.stream.is_some() {
                        drop(self.stream.take());
                        println!("[Audio] Stream Stopped.");
                    }
                }
            }
        }
    }

    fn create_stream(&self) -> Result<cpal::Stream, String> {
        let host = cpal::default_host();
        let device = host.default_input_device()
            .ok_or("No input device found")?;
            
        info!("[Audio] Device: {}", device.name().unwrap_or_default());

        let config: cpal::StreamConfig = device.default_input_config()
            .map_err(|e| format!("Default config error: {}", e))?
            .into();

        let core_tx = self.core_tx.clone();
        let err_fn = move |err| error!("[Audio] Stream Error: {}", err);
        
        let stream = device.build_input_stream(
            &config,
            move |data: &[f32], _: &_| {
                let chunk = data.to_vec();
                let evt = Event::Input(InputEvent {
                    source: "Mic".to_string(),
                    content: InputContent::AudioChunk(chunk)
                });
                
                // Use try_send to avoid blocking audio thread
                if let Err(_e) = core_tx.try_send(evt) {
                    // warn!("[Audio] Drop");
                }
            },
            err_fn,
            None
        ).map_err(|e| format!("{}", e))?;

        stream.play().map_err(|e| format!("{}", e))?;
        Ok(stream)
    }
}
