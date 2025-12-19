use nexus::kernel::audio::monitor::AudioMonitor;
use nexus::kernel::event::AudioSignal;

#[test]
fn test_adaptive_thresholding() {
    let mut monitor = AudioMonitor::new(48000);
    
    // 1. Normal Mode (Baseline 0.03)
    // Low energy noise -> No Signal
    let silence = vec![0.01; 480]; // RMS 0.01 < 0.03
    assert_eq!(monitor.process(&silence), None, "Silence should be ignored");
    
    // Speech -> Signal
    // We need enough duration (120ms)
    // 480 samples = 10ms. Need 12+ chunks.
    let speech = vec![0.1; 480]; // RMS 0.1 > 0.03
    for _ in 0..11 {
        assert_eq!(monitor.process(&speech), None, "Accumulating speech...");
    }
    assert_eq!(monitor.process(&speech), Some(AudioSignal::SpeechStart), "Should trigger SpeechStart");
    
    // Reset state manually or by silence
    let long_silence = vec![0.0; 48000]; // 1s
    monitor.process(&long_silence); // Trigger End
    
    println!("Step 1 passed: Normal sensitivity works.");

    // 2. System Speaking (Protected Mode)
    monitor.set_system_speaking(true);
    
    // Echo (Medium energy, e.g. 0.08)
    // Baseline 0.03 * 3.0 = 0.09 Threshold
    let echo = vec![0.08; 480]; 
    for _ in 0..20 {
        assert_eq!(monitor.process(&echo), None, "Echo (0.08) should be ignored when System Speaking (Thresh 0.09)");
    }
    
    // User Shout (High energy, e.g. 0.2)
    let shout = vec![0.2; 480];
    for _ in 0..11 {
        monitor.process(&shout);
    }
    assert_eq!(monitor.process(&shout), Some(AudioSignal::SpeechStart), "Shout (0.2) should punch through");
    
    // Reset
    monitor.process(&long_silence);
    
    println!("Step 2 passed: System Speaking raises threshold.");

    // 3. Grace Period (Echo Tail)
    monitor.set_system_speaking(false); 
    // Now in Grace Period (300ms)
    
    // Echo tail (0.08) should still be ignored
    for _ in 0..20 {
        assert_eq!(monitor.process(&echo), None, "Echo tail should be ignored in Grace Period");
    }
    
    println!("Step 3 passed: Grace Period protects against tails.");

    // 4. Recovery (After Grace Period)
    // Advance internal time by feeding silence for >300ms
    // 24000 samples = 500ms
    let passage_of_time = vec![0.0; 24000];
    monitor.process(&passage_of_time);
    
    // Now small speech (0.05) should work again (Baseline 0.03)
    let soft_speech = vec![0.05; 480];
    for _ in 0..11 {
        monitor.process(&soft_speech);
    }
    assert_eq!(monitor.process(&soft_speech), Some(AudioSignal::SpeechStart), "Should recover normal sensitivity");

    println!("Step 4 passed: Recovery successful.");
}
