use clap::Parser;
use tracing_subscriber::fmt::init;

#[derive(Parser)]
#[command(name = "fc-evaluator")]
#[command(about = "CI Evaluator - Git polling and Nix evaluation")]
struct Cli {
    #[arg(short, long)]
    config: Option<String>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    #[allow(unused_variables, reason = "Main application logic is TODO")]
    let cli = Cli::parse();

    tracing::info!("Starting CI Evaluator");
    init();

    // TODO: Implement evaluator logic

    Ok(())
}
