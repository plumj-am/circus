//! User repository - CRUD operations and authentication

use sqlx::PgPool;
use uuid::Uuid;

use crate::{
  error::{CiError, Result},
  models::{CreateUser, LoginCredentials, UpdateUser, User},
  roles::{ROLE_READ_ONLY, VALID_ROLES, is_valid_role},
  validation::{
    validate_email,
    validate_full_name,
    validate_password,
    validate_role,
    validate_username,
  },
};

/// Hash a password using argon2id
pub fn hash_password(password: &str) -> Result<String> {
  use argon2::{
    Argon2,
    PasswordHasher,
    password_hash::{SaltString, rand_core::OsRng},
  };

  let salt = SaltString::generate(&mut OsRng);
  let argon2 = Argon2::default();
  argon2
    .hash_password(password.as_bytes(), &salt)
    .map(|h| h.to_string())
    .map_err(|e| CiError::Internal(format!("Password hashing failed: {}", e)))
}

/// Verify a password against a hash
pub fn verify_password(password: &str, hash: &str) -> Result<bool> {
  use argon2::{Argon2, PasswordHash, PasswordVerifier};

  let parsed_hash = PasswordHash::new(hash)
    .map_err(|e| CiError::Internal(format!("Invalid password hash: {}", e)))?;
  let argon2 = Argon2::default();
  Ok(
    argon2
      .verify_password(password.as_bytes(), &parsed_hash)
      .is_ok(),
  )
}

/// Create a new user with validation
pub async fn create(pool: &PgPool, data: &CreateUser) -> Result<User> {
  // Validate username
  validate_username(&data.username)
    .map_err(|e| CiError::Validation(e.to_string()))?;

  // Validate email
  validate_email(&data.email)
    .map_err(|e| CiError::Validation(e.to_string()))?;

  // Validate password
  validate_password(&data.password)
    .map_err(|e| CiError::Validation(e.to_string()))?;

  // Validate full name if provided
  if let Some(ref name) = data.full_name {
    validate_full_name(name).map_err(|e| CiError::Validation(e.to_string()))?;
  }

  // Validate role
  let role = data.role.as_deref().unwrap_or(ROLE_READ_ONLY);
  validate_role(role, VALID_ROLES)
    .map_err(|e| CiError::Validation(e.to_string()))?;

  let password_hash = hash_password(&data.password)?;

  sqlx::query_as::<_, User>(
    "INSERT INTO users (username, email, full_name, password_hash, role) \
     VALUES ($1, $2, $3, $4, $5) RETURNING *",
  )
  .bind(&data.username)
  .bind(&data.email)
  .bind(&data.full_name)
  .bind(&password_hash)
  .bind(role)
  .fetch_one(pool)
  .await
  .map_err(|e| {
    match &e {
      sqlx::Error::Database(db_err) if db_err.is_unique_violation() => {
        CiError::Conflict("Username or email already exists".to_string())
      },
      _ => CiError::Database(e),
    }
  })
}

/// Authenticate a user with username and password
pub async fn authenticate(
  pool: &PgPool,
  creds: &LoginCredentials,
) -> Result<User> {
  let user = sqlx::query_as::<_, User>(
    "SELECT * FROM users WHERE username = $1 AND enabled = true",
  )
  .bind(&creds.username)
  .fetch_one(pool)
  .await
  .map_err(|_| CiError::Unauthorized("Invalid credentials".to_string()))?;

  if let Some(ref hash) = user.password_hash {
    if verify_password(&creds.password, hash)? {
      // Update last login time
      let _ =
        sqlx::query("UPDATE users SET last_login_at = NOW() WHERE id = $1")
          .bind(user.id)
          .execute(pool)
          .await;
      Ok(user)
    } else {
      Err(CiError::Unauthorized("Invalid credentials".to_string()))
    }
  } else {
    Err(CiError::Unauthorized(
      "OAuth user - use OAuth login".to_string(),
    ))
  }
}

/// Get a user by ID
pub async fn get(pool: &PgPool, id: Uuid) -> Result<User> {
  sqlx::query_as::<_, User>("SELECT * FROM users WHERE id = $1")
    .bind(id)
    .fetch_one(pool)
    .await
    .map_err(|e| {
      match e {
        sqlx::Error::RowNotFound => {
          CiError::NotFound(format!("User {} not found", id))
        },
        _ => CiError::Database(e),
      }
    })
}

/// Get a user by username
pub async fn get_by_username(
  pool: &PgPool,
  username: &str,
) -> Result<Option<User>> {
  sqlx::query_as::<_, User>("SELECT * FROM users WHERE username = $1")
    .bind(username)
    .fetch_optional(pool)
    .await
    .map_err(CiError::Database)
}

/// Get a user by email
pub async fn get_by_email(pool: &PgPool, email: &str) -> Result<Option<User>> {
  sqlx::query_as::<_, User>("SELECT * FROM users WHERE email = $1")
    .bind(email)
    .fetch_optional(pool)
    .await
    .map_err(CiError::Database)
}

/// List all users with pagination
pub async fn list(pool: &PgPool, limit: i64, offset: i64) -> Result<Vec<User>> {
  sqlx::query_as::<_, User>(
    "SELECT * FROM users ORDER BY created_at DESC LIMIT $1 OFFSET $2",
  )
  .bind(limit)
  .bind(offset)
  .fetch_all(pool)
  .await
  .map_err(CiError::Database)
}

/// Count total users
pub async fn count(pool: &PgPool) -> Result<i64> {
  let (count,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM users")
    .fetch_one(pool)
    .await?;
  Ok(count)
}

/// Update a user with the provided data
pub async fn update(
  pool: &PgPool,
  id: Uuid,
  data: &UpdateUser,
) -> Result<User> {
  // Apply all updates sequentially
  if let Some(ref email) = data.email {
    update_email(pool, id, email).await?;
  }

  if let Some(ref full_name) = data.full_name {
    update_full_name(pool, id, Some(full_name.as_str())).await?;
  }

  if let Some(ref password) = data.password {
    update_password(pool, id, password).await?;
  }

  if let Some(ref role) = data.role {
    update_role(pool, id, role).await?;
  }

  if let Some(enabled) = data.enabled {
    set_enabled(pool, id, enabled).await?;
  }

  if let Some(public) = data.public_dashboard {
    set_public_dashboard(pool, id, public).await?;
  }

  get(pool, id).await
}

/// Update user email with validation
pub async fn update_email(
  pool: &PgPool,
  id: Uuid,
  email: &str,
) -> Result<User> {
  validate_email(email).map_err(|e| CiError::Validation(e.to_string()))?;

  sqlx::query_as::<_, User>(
    "UPDATE users SET email = $1 WHERE id = $2 RETURNING *",
  )
  .bind(email)
  .bind(id)
  .fetch_one(pool)
  .await
  .map_err(|e| {
    match &e {
      sqlx::Error::Database(db_err) if db_err.is_unique_violation() => {
        CiError::Conflict("Email already in use".to_string())
      },
      _ => CiError::Database(e),
    }
  })
}

/// Update user full name with validation
pub async fn update_full_name(
  pool: &PgPool,
  id: Uuid,
  full_name: Option<&str>,
) -> Result<()> {
  if let Some(name) = full_name {
    validate_full_name(name).map_err(|e| CiError::Validation(e.to_string()))?;
  }

  sqlx::query("UPDATE users SET full_name = $1 WHERE id = $2")
    .bind(full_name)
    .bind(id)
    .execute(pool)
    .await?;
  Ok(())
}

/// Update user password with validation
pub async fn update_password(
  pool: &PgPool,
  id: Uuid,
  password: &str,
) -> Result<()> {
  validate_password(password)
    .map_err(|e| CiError::Validation(e.to_string()))?;

  let hash = hash_password(password)?;
  sqlx::query("UPDATE users SET password_hash = $1 WHERE id = $2")
    .bind(&hash)
    .bind(id)
    .execute(pool)
    .await?;
  Ok(())
}

/// Update user role with validation
pub async fn update_role(pool: &PgPool, id: Uuid, role: &str) -> Result<()> {
  validate_role(role, VALID_ROLES)
    .map_err(|e| CiError::Validation(e.to_string()))?;

  sqlx::query("UPDATE users SET role = $1 WHERE id = $2")
    .bind(role)
    .bind(id)
    .execute(pool)
    .await?;
  Ok(())
}

/// Enable/disable user
pub async fn set_enabled(pool: &PgPool, id: Uuid, enabled: bool) -> Result<()> {
  sqlx::query("UPDATE users SET enabled = $1 WHERE id = $2")
    .bind(enabled)
    .bind(id)
    .execute(pool)
    .await?;
  Ok(())
}

/// Set public dashboard preference
pub async fn set_public_dashboard(
  pool: &PgPool,
  id: Uuid,
  public: bool,
) -> Result<()> {
  sqlx::query("UPDATE users SET public_dashboard = $1 WHERE id = $2")
    .bind(public)
    .bind(id)
    .execute(pool)
    .await?;
  Ok(())
}

/// Delete a user
pub async fn delete(pool: &PgPool, id: Uuid) -> Result<()> {
  let result = sqlx::query("DELETE FROM users WHERE id = $1")
    .bind(id)
    .execute(pool)
    .await?;
  if result.rows_affected() == 0 {
    return Err(CiError::NotFound(format!("User {} not found", id)));
  }
  Ok(())
}

/// Create or update OAuth user
pub async fn upsert_oauth_user(
  pool: &PgPool,
  username: &str,
  email: &str,
  full_name: Option<&str>,
  user_type: &str,
) -> Result<User> {
  sqlx::query_as::<_, User>(
    "INSERT INTO users (username, email, full_name, user_type, password_hash) \
     VALUES ($1, $2, $3, $4, NULL) ON CONFLICT (username) DO UPDATE SET email \
     = EXCLUDED.email, full_name = EXCLUDED.full_name, updated_at = NOW() \
     RETURNING *",
  )
  .bind(username)
  .bind(email)
  .bind(full_name)
  .bind(user_type)
  .fetch_one(pool)
  .await
  .map_err(|e| {
    match &e {
      sqlx::Error::Database(db_err) if db_err.is_unique_violation() => {
        CiError::Conflict("Email already in use".to_string())
      },
      _ => CiError::Database(e),
    }
  })
}
