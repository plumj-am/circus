//! Database migration utilities

use sqlx::{PgPool, Postgres, migrate::MigrateDatabase};
use tracing::{error, info, warn};

/// Runs database migrations and ensures the database exists
pub async fn run_migrations(database_url: &str) -> anyhow::Result<()> {
    info!("Starting database migrations");

    // Check if database exists, create if it doesn't
    if !Postgres::database_exists(database_url).await? {
        warn!("Database does not exist, creating it");
        Postgres::create_database(database_url).await?;
        info!("Database created successfully");
    }

    // Set up connection pool with retry logic, then run migrations
    let pool = create_connection_pool(database_url).await?;
    match sqlx::migrate!("./migrations").run(&pool).await {
        Ok(()) => {
            info!("Database migrations completed successfully");
            Ok(())
        }
        Err(e) => {
            error!("Failed to run database migrations: {}", e);
            Err(anyhow::anyhow!("Migration failed: {e}"))
        }
    }
}

/// Creates a connection pool with proper configuration
async fn create_connection_pool(database_url: &str) -> anyhow::Result<PgPool> {
    let pool = PgPool::connect(database_url).await?;

    // Test the connection
    sqlx::query("SELECT 1").fetch_one(&pool).await?;

    Ok(pool)
}

/// Validates that all required tables exist and have the expected structure
pub async fn validate_schema(pool: &PgPool) -> anyhow::Result<()> {
    info!("Validating database schema");

    let required_tables = vec![
        "projects",
        "jobsets",
        "evaluations",
        "builds",
        "build_products",
        "build_steps",
    ];

    for table in required_tables {
        let result = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM information_schema.tables WHERE table_name = $1",
        )
        .bind(table)
        .fetch_one(pool)
        .await?;

        if result == 0 {
            return Err(anyhow::anyhow!("Required table '{table}' does not exist"));
        }
    }

    info!("Database schema validation passed");
    Ok(())
}
