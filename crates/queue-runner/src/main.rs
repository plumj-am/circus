use clap::Parser;
use tracing_subscriber::fmt::init;

#[derive(Parser)]
#[command(name = "fc-queue-runner")]
#[command(about = "CI Queue Runner - Build dispatch and execution")]
struct Cli {
    #[arg(short, long)]
    workers: Option<usize>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    #[allow(unused_variables, reason = "Main application logic is TODO")]
    let cli = Cli::parse();

    tracing::info!("Starting CI Queue Runner");
    init();

    // TODO: Implement queue runner logic

    Ok(())
}
