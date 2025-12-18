use ringbuf::traits::Consumer;
use tokio::sync::mpsc;
use crate::kernel::event::{Event, InputEvent, AudioSignal, InputContent};
use tracing::{info, debug};
// use webrtc_vad::{Vad, SampleRate}; // Depending on crate version api

pub struct AudioProcessor<C> 
where C: Consumer<Item = f32> + Send
{
    consumer: C,
    tx: mpsc::Sender<Event>,
    sample_rate: u32,
    
    // State
    is_speaking: bool,
    consecutive_speech: usize,
    consecutive_silence: usize,
}

impl<C> AudioProcessor<C>
where C: Consumer<Item = f32> + Send
{
    pub fn new(consumer: C, tx: mpsc::Sender<Event>, sample_rate: u32) -> Self {
        Self {
            consumer,
            tx,
            sample_rate,
            is_speaking: false,
            consecutive_speech: 0,
            consecutive_silence: 0,
        }
    }

    pub fn run(mut self) {
        info!("Audio Processor Started. Rate: {}Hz", self.sample_rate);
        
        // VAD Config
        let mut vad = webrtc_vad::Vad::new();
        // Mode 3 is very aggressive (suppress noise).
        vad.set_mode(webrtc_vad::VadMode::Aggressive);

        // Frame duration 30ms
        let frame_ms = 30;
        let frame_size = (self.sample_rate as usize * frame_ms) / 1000;
        
        // If sample rate not supported, log error and exit
        match self.sample_rate {
             8000 | 16000 | 32000 | 48000 => {},
             _ => {
                 debug!("Unsupported VAD Rate {}", self.sample_rate);
                 return;
             }
        }

        let mut frame_buf_f32: Vec<f32> = vec![0.0; frame_size];
        let mut frame_buf_i16: Vec<i16> = vec![0; frame_size];

        // Tuning parameters
        let min_speech_frames = 3;  // 90ms to trigger start
        let min_silence_frames = 20; // 600ms to trigger end

        loop {
            // 1. Read Frame
            // We need a full frame. If missing, sleep briefly.
            if self.consumer.occupied_len() < frame_size {
                std::thread::sleep(std::time::Duration::from_millis(10));
                continue;
            }

            // Pop samples
            let _ = self.consumer.pop_slice(&mut frame_buf_f32);
            
            // 2. Convert f32 -> i16
            for (i, &sample) in frame_buf_f32.iter().enumerate() {
                frame_buf_i16[i] = (sample * i16::MAX as f32) as i16;
            }

            // 3. VAD
            let is_speech_frame = match vad.is_voice_segment(&frame_buf_i16) {
                Ok(res) => res,
                Err(e) => {
                    // Log once or debug?
                    debug!("VAD Error: {:?}", e);
                    false
                }
            };

            // 4. Debounce / State Machine
            if is_speech_frame {
                self.consecutive_silence = 0;
                self.consecutive_speech += 1;
            } else {
                self.consecutive_speech = 0;
                self.consecutive_silence += 1;
            }

            if !self.is_speaking && self.consecutive_speech >= min_speech_frames {
                self.is_speaking = true;
                info!("Audio Control: Speech START detected");
                let _ = self.tx.blocking_send(Event::Input(InputEvent {
                    source: "Audio".to_string(),
                    content: InputContent::Audio(AudioSignal::SpeechStart),
                }));
            } else if self.is_speaking && self.consecutive_silence >= min_silence_frames {
                self.is_speaking = false;
                info!("Audio Control: Speech END detected");
                let _ = self.tx.blocking_send(Event::Input(InputEvent {
                    source: "Audio".to_string(),
                    content: InputContent::Audio(AudioSignal::SpeechEnd),
                }));
            }
        }
    }
}
