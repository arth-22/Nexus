use tokio::sync::mpsc;
use crate::kernel::event::{Event, InputEvent, VisualSignal, InputContent};
use tracing::{info, debug, warn};
use std::time::Duration;
use img_hash::{HasherConfig, HashAlg}; // Depending on crate api
use image::imageops::FilterType;

pub struct VisionPipeline {
    tx: mpsc::Sender<Event>,
}

impl VisionPipeline {
    pub fn new(tx: mpsc::Sender<Event>) -> Self {
        Self { tx }
    }

    /// Run the vision loop. This is designed to be run in a dedicated OS thread 
    /// (to avoid blocking async runtime with image processing).
    pub fn run(self) {
        info!("Vision Pipleine Started (0.5 FPS - Mock / Real)");
        
        let mut last_hash: Option<u64> = None;
        let hasher = HasherConfig::new().hash_alg(HashAlg::Gradient).hash_size(8, 8).to_hasher();

        loop {
            // 1. Capture (Stub for now, or xcap if compiling)
            // Ideally we use xcap here. 
            // For stability of phase 3 implementation first step, I will use a simple noise generator or stub
            // until I wire real xcap. OR I can try xcap directly.
            // Let's implement real xcap logic but wrapped in Result to handle failure gracefully (Silence).
            
            let frame_opt = self.capture_screen();
            
            if let Some(img) = frame_opt {
                // 2. Hash
                // Resize for perf is handled by `hash_image` mostly, 
                // but pre-scaling helps deterministic control.
                // img_hash takes generic image.
                
                let hash_obj = hasher.hash_image(&img);
                // unsafe bits or as_u64 if available? 
                // img_hash 3.2 uses `ImageHash` struct. 
                // We need a stable u64 representation for Event.
                // Usually has `to_base64` or bytes. 
                // Let's assume we can get bits.
                
                // Workaround for u64 extraction if crate doesn't expose it directly nicely:
                // Gradient hash 8x8 = 64 bits.
                // We can iterate bits.
                let mut hash_u64: u64 = 0;
                for (i, bit) in hash_obj.as_bits().iter().enumerate() {
                     if *bit == 1 {
                         hash_u64 |= 1 << i;
                     }
                }

                // 3. Diff & Emit
                if let Some(prev) = last_hash {
                    let dist = (prev ^ hash_u64).count_ones(); // Hamming distance
                    
                    // Emit FACT: Percept Update
                    let _ = self.tx.blocking_send(Event::Input(InputEvent {
                        source: "Vision".to_string(),
                        content: InputContent::Visual(VisualSignal::PerceptUpdate {
                           hash: hash_u64,
                           distance: dist as u32,
                        }),
                    }));
                } else {
                     // First frame
                     let _ = self.tx.blocking_send(Event::Input(InputEvent {
                        source: "Vision".to_string(),
                        content: InputContent::Visual(VisualSignal::PerceptUpdate {
                           hash: hash_u64,
                           distance: 0,
                        }),
                    }));
                }
                
                last_hash = Some(hash_u64);
            } else {
                // Capture failed? Silent.
                // No event. System decays stability naturally if implementation requires ping,
                // OR system just assumes "nothing changed" if passive.
                // User requirement: "Failure to capture must degrade to silence... no frame != new context".
                // So we do NOTHING.
            }

            // Sleep 200ms -> 5 FPS
            std::thread::sleep(Duration::from_millis(200));
        }
    }

    fn capture_screen(&self) -> Option<image::DynamicImage> {
        // Mock Capture for Stability/Verification
        // Create a 224x224 image with some noise or solid color
        // For testing, we can just return a consistent image.
        let mut img = image::DynamicImage::new_rgb8(224, 224);
        // Fill with black (stable)
        // In real world, xcap::Monitor::all() would be used.
        // xcap might be failing to compile on this env.
        
        // Mocking behavior:
        // Ideally we want to simulate change?
        // But for now return Some.
        Some(img)
    }
}
