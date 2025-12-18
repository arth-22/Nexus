use tracing_subscriber::{FmtSubscriber, EnvFilter};
use tokio::io::{AsyncBufReadExt, BufReader};
use nexus::kernel::event::{Event, InputEvent};
use nexus::kernel::reactor::Reactor;
use tokio::sync::mpsc;

#[tokio::main]
async fn main() {
    // 1. Setup Logging
    let subscriber = FmtSubscriber::builder()
        .with_env_filter(EnvFilter::from_default_env())
        .finish();
    tracing::subscriber::set_global_default(subscriber).expect("setting default subscriber failed");

    tracing::info!("Starting Live Nexus Verification Kernel...");

    // 2. Setup Reactor + Channels
    let (tx_input, rx_input) = mpsc::channel(100);
    // Note: We need a way to clone sender for Audio Processor.
    // mpsc::Sender is clonable.
    let tx_audio = tx_input.clone();

    // 3. Setup Audio Stack
    use ringbuf::traits::Split;
    use ringbuf::HeapRb;
    
    // Buffer size: 16k sample rate * 0.1s = 1600 samples. 
    // Let's give it plenty of room (0.5s) to avoid buffer overflow during GC/jitters
    let rb = HeapRb::<f32>::new(8192);
    let (producer, consumer) = rb.split();


    // Wait, capture chooses rate dynamically. We need to pass the ACTUAL rate.
    // AudioCapture::new returns the struct which has the rate.
    // BUT AudioCapture creation is blocking inside that thread. 
    // We need to coordinate.
    // Simpler: Create Capture in Main, then move stream to thread or just hold it?
    // CPAL stream is `Send`.
    
    // let capture = nexus::audio::capture::AudioCapture::new(producer).expect("Failed to init audio");
    // let rate = capture.sample_rate;
    // tokio::spawn(nexus::audio::processing::AudioProcessor::new(consumer, tx_audio, rate).run());
    // This works if `AudioCapture` is created here.
    
    let capture = nexus::audio::capture::AudioCapture::new(producer)
        .expect("Failed to initialize Audio Capture");
    let rate = capture.sample_rate;
    tracing::info!("Audio Capture Initialized at {}Hz", rate);
    
    std::thread::spawn(move || {
        nexus::audio::processing::AudioProcessor::new(consumer, tx_audio, rate).run();
    });
    
    // The capture struct holds the stream. It must be kept alive.
    // We can move it into a task that just waits, or keep it in main scope.
    // Main loop runs 'reactor.run()'. If we keep `capture` in a variable here, it lives until main ends.
    // That is sufficient.

    let mut reactor = Reactor::new(rx_input, tx_input.clone());
    
    // 4. Spawn Input Reader (Stdin)
    tokio::spawn(async move {
        let stdin = tokio::io::stdin();
        let reader = BufReader::new(stdin);
        let mut lines = reader.lines();
        
        println!("Type 'Hello' to trigger interaction, or 'Stop' to interrupt.");
        
        while let Ok(Some(line)) = lines.next_line().await {
            if line.trim().is_empty() { continue; }
            
            let event = Event::Input(InputEvent::text("console", &line));
            tracing::info!("Console Input Dispatched: '{}'", line);
            
            if let Err(e) = tx_input.send(event).await {
                tracing::error!("Failed to send input: {}", e);
                break;
            }
        }
    });

    // 5. Run Kernel
    tracing::info!("Kernel Loop Active.");
    reactor.run().await;
}
