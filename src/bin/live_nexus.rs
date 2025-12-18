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

    // 2. Setup Channels
    let (tx, rx) = mpsc::channel(100);
    let tx_input = tx.clone(); // For stdin loop

    // 3. Setup Reactor
    let mut reactor = Reactor::new(rx, tx);

    // 4. Spawn Input Reader (Stdin)
    tokio::spawn(async move {
        let stdin = tokio::io::stdin();
        let reader = BufReader::new(stdin);
        let mut lines = reader.lines();
        
        println!("Type 'Hello' to trigger interaction, or 'Stop' to interrupt.");
        
        while let Ok(Some(line)) = lines.next_line().await {
            if line.trim().is_empty() { continue; }
            
            let event = Event::Input(InputEvent {
                source: "console".to_string(),
                content: line.clone(),
            });
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
