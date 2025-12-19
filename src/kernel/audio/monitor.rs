use crate::kernel::event::AudioSignal;

/// Simple Energy-Based VAD (Voice Activity Detection)
/// Phase D Requirement: Signal analysis only. No ASR.
pub struct AudioMonitor {
    // Configuration
    sample_rate: u32,
    threshold_rms: f32, // Energy threshold
    min_speech_duration_ms: u64,
    min_silence_duration_ms: u64,

    // Configuration
    adaptive_threshold_factor: f32, // Multiplier when system is speaking
    grace_period_ms: u64,           // Echo tail protection window

    // State
    is_speaking: bool,
    consecutive_prob_speech: u64, // accumulated duration > threshold
    consecutive_prob_silence: u64, // accumulated duration < threshold
    
    // Phase F: Output Awareness
    system_speaking: bool,
    playback_end_tick: Option<u64>, // Monotonic MS timestamp
    current_time_ms: u64, // Monotonic MS counter (estimated from samples)
}

impl AudioMonitor {
    pub fn new(sample_rate: u32) -> Self {
        Self {
            sample_rate,
            threshold_rms: 0.03, // Increased from 0.015 to reduce echo sensitivity
            min_speech_duration_ms: 120, // Increased to 120ms (ignore short pops)
            min_silence_duration_ms: 500, // 0.5s pause to cut
            
            adaptive_threshold_factor: 3.0, // 3x threshold when Nexus is speaking
            grace_period_ms: 300,           // 300ms tail protection
            
            is_speaking: false,
            consecutive_prob_speech: 0,
            consecutive_prob_silence: 0,
            
            system_speaking: false,
            playback_end_tick: None,
            current_time_ms: 0,
        }
    }

    pub fn set_system_speaking(&mut self, speaking: bool) {
        if self.system_speaking && !speaking {
            // Transition to Silent -> Mark end time
            self.playback_end_tick = Some(self.current_time_ms);
        }
        self.system_speaking = speaking;
    }

    /// Process a chunk of raw audio float samples.
    /// Returns Some(Signal) if a state transition occurs.
    pub fn process(&mut self, samples: &[f32]) -> Option<AudioSignal> {
        if samples.is_empty() {
            return None;
        }

        // 1. Calculate RMS Energy
        let sq_sum: f32 = samples.iter().map(|&x| x * x).sum();
        let rms = (sq_sum / samples.len() as f32).sqrt();

        // 2. State Machine
        let chunk_duration_ms = (samples.len() as u64 * 1000) / self.sample_rate as u64;
        self.current_time_ms += chunk_duration_ms;

        // Phase F: Adaptive Threshold Logic
        let effective_threshold = if self.system_speaking {
            self.threshold_rms * self.adaptive_threshold_factor
        } else if let Some(end_tick) = self.playback_end_tick {
             if self.current_time_ms.saturating_sub(end_tick) < self.grace_period_ms {
                 self.threshold_rms * self.adaptive_threshold_factor
             } else {
                 self.threshold_rms
             }
        } else {
            self.threshold_rms
        };

        if rms > effective_threshold {
            // High Energy
            self.consecutive_prob_speech += chunk_duration_ms;
            self.consecutive_prob_silence = 0;
            
            if !self.is_speaking && self.consecutive_prob_speech >= self.min_speech_duration_ms {
                self.is_speaking = true;
                return Some(AudioSignal::SpeechStart);
            }
        } else {
            // Low Energy (Silence)
            self.consecutive_prob_silence += chunk_duration_ms;
            self.consecutive_prob_speech = 0;

            if self.is_speaking && self.consecutive_prob_silence >= self.min_silence_duration_ms {
                self.is_speaking = false;
                return Some(AudioSignal::SpeechEnd);
            }
        }

        None
    }
}
