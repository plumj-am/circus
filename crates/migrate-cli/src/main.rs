//! Database migration CLI utility

use fc_common::migrate_cli::run;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    run().await
}
