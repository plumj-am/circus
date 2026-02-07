//! Declarative bootstrap: upsert projects, jobsets, API keys and users from
//! config.
//!
//! Called once on server startup to reconcile declarative configuration
//! with database state. Uses upsert semantics so repeated runs are idempotent.

use sha2::{Digest, Sha256};
use sqlx::PgPool;

use crate::{
  config::DeclarativeConfig,
  error::Result,
  models::{CreateJobset, CreateProject},
  repo,
};

/// Bootstrap declarative configuration into the database.
///
/// This function is idempotent: running it multiple times with the same config
/// produces the same database state. It upserts (insert or update) all
/// configured projects, jobsets, API keys, and users.
pub async fn run(pool: &PgPool, config: &DeclarativeConfig) -> Result<()> {
  if config.projects.is_empty()
    && config.api_keys.is_empty()
    && config.users.is_empty()
  {
    return Ok(());
  }

  let n_projects = config.projects.len();
  let n_jobsets: usize = config.projects.iter().map(|p| p.jobsets.len()).sum();
  let n_keys = config.api_keys.len();
  let n_users = config.users.len();

  tracing::info!(
    projects = n_projects,
    jobsets = n_jobsets,
    api_keys = n_keys,
    users = n_users,
    "Bootstrapping declarative configuration"
  );

  // Upsert projects and their jobsets
  for decl_project in &config.projects {
    let project = repo::projects::upsert(pool, CreateProject {
      name:           decl_project.name.clone(),
      repository_url: decl_project.repository_url.clone(),
      description:    decl_project.description.clone(),
    })
    .await?;

    tracing::info!(
        project = %project.name,
        id = %project.id,
        "Upserted declarative project"
    );

    for decl_jobset in &decl_project.jobsets {
      let jobset = repo::jobsets::upsert(pool, CreateJobset {
        project_id:        project.id,
        name:              decl_jobset.name.clone(),
        nix_expression:    decl_jobset.nix_expression.clone(),
        enabled:           Some(decl_jobset.enabled),
        flake_mode:        Some(decl_jobset.flake_mode),
        check_interval:    Some(decl_jobset.check_interval),
        branch:            None,
        scheduling_shares: None,
        state:             None,
      })
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
      repo::api_keys::upsert(pool, &decl_key.name, &key_hash, &decl_key.role)
        .await?;

    tracing::info!(
        name = %api_key.name,
        role = %api_key.role,
        "Upserted declarative API key"
    );
  }

  // Upsert users
  for decl_user in &config.users {
    // Resolve password from inline or file
    let password = if let Some(ref p) = decl_user.password {
      Some(p.clone())
    } else if let Some(ref file) = decl_user.password_file {
      match std::fs::read_to_string(file) {
        Ok(p) => Some(p.trim().to_string()),
        Err(e) => {
          tracing::warn!(
            username = %decl_user.username,
            file = %file,
            "Failed to read password file: {e}"
          );
          None
        },
      }
    } else {
      None
    };

    // Check if user exists
    let existing =
      repo::users::get_by_username(pool, &decl_user.username).await?;

    if let Some(user) = existing {
      // Update existing user
      let update = crate::models::UpdateUser {
        email:            Some(decl_user.email.clone()),
        full_name:        decl_user.full_name.clone(),
        password,
        role:             Some(decl_user.role.clone()),
        enabled:          Some(decl_user.enabled),
        public_dashboard: None,
      };
      if let Err(e) = repo::users::update(pool, user.id, &update).await {
        tracing::warn!(
          username = %decl_user.username,
          "Failed to update declarative user: {e}"
        );
      } else {
        tracing::info!(
          username = %decl_user.username,
          "Updated declarative user"
        );
      }
    } else if let Some(pwd) = password {
      // Create new user
      let create = crate::models::CreateUser {
        username:  decl_user.username.clone(),
        email:     decl_user.email.clone(),
        full_name: decl_user.full_name.clone(),
        password:  pwd,
        role:      Some(decl_user.role.clone()),
      };
      match repo::users::create(pool, &create).await {
        Ok(user) => {
          tracing::info!(
            username = %user.username,
            "Created declarative user"
          );
          // Set enabled status if false (users are enabled by default)
          if !decl_user.enabled
            && let Err(e) = repo::users::set_enabled(pool, user.id, false).await
            {
              tracing::warn!(
                username = %user.username,
                "Failed to disable declarative user: {e}"
              );
            }
        },
        Err(e) => {
          tracing::warn!(
            username = %decl_user.username,
            "Failed to create declarative user: {e}"
          );
        },
      }
    } else {
      tracing::warn!(
        username = %decl_user.username,
        "Declarative user has no password set, skipping creation"
      );
    }
  }

  tracing::info!("Declarative bootstrap complete");
  Ok(())
}
