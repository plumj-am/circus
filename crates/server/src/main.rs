use clap::Parser;
use tracing_subscriber::fmt::init;

#[derive(Parser)]
#[command(name = "fc-server")]
#[command(about = "CI Server - Web API and UI")]
struct Cli {
    #[arg(short, long, default_value = "3000")]
    port: u16,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    tracing::info!("Starting CI Server on port {}", cli.port);
    init();

    // TODO: Implement server logic

    Ok(())
}
