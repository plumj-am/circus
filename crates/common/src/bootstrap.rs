//! Declarative bootstrap: upsert projects, jobsets, and API keys from config.
//!
//! Called once on server startup to reconcile declarative configuration
//! with database state. Uses upsert semantics so repeated runs are idempotent.

use sha2::{Digest, Sha256};
use sqlx::PgPool;

use crate::config::DeclarativeConfig;
use crate::error::Result;
use crate::models::{CreateJobset, CreateProject};
use crate::repo;

/// Bootstrap declarative configuration into the database.
///
/// This function is idempotent: running it multiple times with the same config
/// produces the same database state. It upserts (insert or update) all
/// configured projects, jobsets, and API keys.
pub async fn run(pool: &PgPool, config: &DeclarativeConfig) -> Result<()> {
    if config.projects.is_empty() && config.api_keys.is_empty() {
        return Ok(());
    }

    let n_projects = config.projects.len();
    let n_jobsets: usize = config.projects.iter().map(|p| p.jobsets.len()).sum();
    let n_keys = config.api_keys.len();

    tracing::info!(
        projects = n_projects,
        jobsets = n_jobsets,
        api_keys = n_keys,
        "Bootstrapping declarative configuration"
    );

    // Upsert projects and their jobsets
    for decl_project in &config.projects {
        let project = repo::projects::upsert(
            pool,
            CreateProject {
                name: decl_project.name.clone(),
                repository_url: decl_project.repository_url.clone(),
                description: decl_project.description.clone(),
            },
        )
        .await?;

        tracing::info!(
            project = %project.name,
            id = %project.id,
            "Upserted declarative project"
        );

        for decl_jobset in &decl_project.jobsets {
            let jobset = repo::jobsets::upsert(
                pool,
                CreateJobset {
                    project_id: project.id,
                    name: decl_jobset.name.clone(),
                    nix_expression: decl_jobset.nix_expression.clone(),
                    enabled: Some(decl_jobset.enabled),
                    flake_mode: Some(decl_jobset.flake_mode),
                    check_interval: Some(decl_jobset.check_interval),
                    branch: None,
                    scheduling_shares: None,
                },
            )
            .await?;

            tracing::info!(
                project = %project.name,
                jobset = %jobset.name,
                "Upserted declarative jobset"
            );
        }
    }

    // Upsert API keys
    for decl_key in &config.api_keys {
        let mut hasher = Sha256::new();
        hasher.update(decl_key.key.as_bytes());
        let key_hash = hex::encode(hasher.finalize());

        let api_key =
            repo::api_keys::upsert(pool, &decl_key.name, &key_hash, &decl_key.role).await?;

        tracing::info!(
            name = %api_key.name,
            role = %api_key.role,
            "Upserted declarative API key"
        );
    }

    tracing::info!("Declarative bootstrap complete");
    Ok(())
}
