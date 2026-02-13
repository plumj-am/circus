//! Tests for the queue runner.
//! Nix log parsing tests require no external binaries.
//! Database tests require TEST_DATABASE_URL.

// Nix log line parsing

#[test]
fn test_parse_nix_log_start() {
  let line =
    r#"@nix {"action":"start","derivation":"/nix/store/abc-hello.drv"}"#;
  let result = fc_queue_runner::builder::parse_nix_log_line(line);
  assert!(result.is_some());
  let (action, drv) = result.unwrap();
  assert_eq!(action, "start");
  assert_eq!(drv, "/nix/store/abc-hello.drv");
}

#[test]
fn test_parse_nix_log_stop() {
  let line =
    r#"@nix {"action":"stop","derivation":"/nix/store/abc-hello.drv"}"#;
  let result = fc_queue_runner::builder::parse_nix_log_line(line);
  assert!(result.is_some());
  let (action, drv) = result.unwrap();
  assert_eq!(action, "stop");
  assert_eq!(drv, "/nix/store/abc-hello.drv");
}

#[test]
fn test_parse_nix_log_unknown_action() {
  let line = r#"@nix {"action":"msg","msg":"building..."}"#;
  let result = fc_queue_runner::builder::parse_nix_log_line(line);
  assert!(result.is_none());
}

#[test]
fn test_parse_nix_log_not_nix_prefix() {
  let line = "building '/nix/store/abc-hello.drv'...";
  let result = fc_queue_runner::builder::parse_nix_log_line(line);
  assert!(result.is_none());
}

#[test]
fn test_parse_nix_log_invalid_json() {
  let line = "@nix {invalid json}";
  let result = fc_queue_runner::builder::parse_nix_log_line(line);
  assert!(result.is_none());
}

#[test]
fn test_parse_nix_log_no_derivation_field() {
  let line = r#"@nix {"action":"start","type":"build"}"#;
  let result = fc_queue_runner::builder::parse_nix_log_line(line);
  assert!(result.is_none());
}

#[test]
fn test_parse_nix_log_empty_line() {
  let result = fc_queue_runner::builder::parse_nix_log_line("");
  assert!(result.is_none());
}

// WorkerPool drain

#[tokio::test]
async fn test_worker_pool_drain_stops_dispatch() {
  // Create a minimal worker pool
  let url = match std::env::var("TEST_DATABASE_URL") {
    Ok(url) => url,
    Err(_) => {
      println!("Skipping: TEST_DATABASE_URL not set");
      return;
    },
  };

  let pool = sqlx::postgres::PgPoolOptions::new()
    .max_connections(1)
    .connect(&url)
    .await
    .expect("failed to connect");

  let worker_pool = fc_queue_runner::worker::WorkerPool::new(
    pool,
    2,
    std::env::temp_dir(),
    std::time::Duration::from_secs(60),
    fc_common::config::LogConfig::default(),
    fc_common::config::GcConfig::default(),
    fc_common::config::NotificationsConfig::default(),
    fc_common::config::SigningConfig::default(),
    fc_common::config::CacheUploadConfig::default(),
    None,
  );

  // Drain should not panic
  worker_pool.drain();

  // After drain, dispatching should be a no-op (build won't start)
  // We can't easily test this without a real build, but at least verify drain
  // doesn't crash
}

// Database-dependent tests

#[tokio::test]
async fn test_atomic_build_claiming() {
  let url = match std::env::var("TEST_DATABASE_URL") {
    Ok(url) => url,
    Err(_) => {
      println!("Skipping: TEST_DATABASE_URL not set");
      return;
    },
  };

  let pool = sqlx::postgres::PgPoolOptions::new()
    .max_connections(5)
    .connect(&url)
    .await
    .expect("failed to connect");

  sqlx::migrate!("../common/migrations")
    .run(&pool)
    .await
    .expect("migration failed");

  // Create a project -> jobset -> evaluation -> build chain
  let project = fc_common::repo::projects::create(
    &pool,
    fc_common::models::CreateProject {
      name:           format!("runner-test-{}", uuid::Uuid::new_v4()),
      description:    None,
      repository_url: "https://github.com/test/repo".to_string(),
    },
  )
  .await
  .expect("create project");

  let jobset =
    fc_common::repo::jobsets::create(&pool, fc_common::models::CreateJobset {
      project_id:        project.id,
      name:              "main".to_string(),
      nix_expression:    "packages".to_string(),
      enabled:           None,
      flake_mode:        None,
      check_interval:    None,
      branch:            None,
      scheduling_shares: None,
      state:             None,
    })
    .await
    .expect("create jobset");

  let eval = fc_common::repo::evaluations::create(
    &pool,
    fc_common::models::CreateEvaluation {
      jobset_id:      jobset.id,
      commit_hash:    "abcdef1234567890abcdef1234567890abcdef12".to_string(),
      pr_number:      None,
      pr_head_branch: None,
      pr_base_branch: None,
      pr_action:      None,
    },
  )
  .await
  .expect("create eval");

  let build =
    fc_common::repo::builds::create(&pool, fc_common::models::CreateBuild {
      evaluation_id: eval.id,
      job_name:      "test-build".to_string(),
      drv_path:      "/nix/store/test-runner-test.drv".to_string(),
      system:        Some("x86_64-linux".to_string()),
      outputs:       None,
      is_aggregate:  None,
      constituents:  None,
    })
    .await
    .expect("create build");

  assert_eq!(build.status, fc_common::models::BuildStatus::Pending);

  // First claim should succeed
  let claimed = fc_common::repo::builds::start(&pool, build.id)
    .await
    .expect("start build");
  assert!(claimed.is_some());

  // Second claim should return None (already claimed)
  let claimed2 = fc_common::repo::builds::start(&pool, build.id)
    .await
    .expect("start build again");
  assert!(claimed2.is_none());

  // Clean up
  let _ = fc_common::repo::projects::delete(&pool, project.id).await;
}

#[tokio::test]
async fn test_orphan_build_reset() {
  let url = match std::env::var("TEST_DATABASE_URL") {
    Ok(url) => url,
    Err(_) => {
      println!("Skipping: TEST_DATABASE_URL not set");
      return;
    },
  };

  let pool = sqlx::postgres::PgPoolOptions::new()
    .max_connections(5)
    .connect(&url)
    .await
    .expect("failed to connect");

  sqlx::migrate!("../common/migrations")
    .run(&pool)
    .await
    .expect("migration failed");

  let project = fc_common::repo::projects::create(
    &pool,
    fc_common::models::CreateProject {
      name:           format!("orphan-test-{}", uuid::Uuid::new_v4()),
      description:    None,
      repository_url: "https://github.com/test/repo".to_string(),
    },
  )
  .await
  .expect("create project");

  let jobset =
    fc_common::repo::jobsets::create(&pool, fc_common::models::CreateJobset {
      project_id:        project.id,
      name:              "main".to_string(),
      nix_expression:    "packages".to_string(),
      enabled:           None,
      flake_mode:        None,
      check_interval:    None,
      branch:            None,
      scheduling_shares: None,
      state:             None,
    })
    .await
    .expect("create jobset");

  let eval = fc_common::repo::evaluations::create(
    &pool,
    fc_common::models::CreateEvaluation {
      jobset_id:      jobset.id,
      commit_hash:    "1234567890abcdef1234567890abcdef12345678".to_string(),
      pr_number:      None,
      pr_head_branch: None,
      pr_base_branch: None,
      pr_action:      None,
    },
  )
  .await
  .expect("create eval");

  // Create a build and mark it running
  let build =
    fc_common::repo::builds::create(&pool, fc_common::models::CreateBuild {
      evaluation_id: eval.id,
      job_name:      "orphan-build".to_string(),
      drv_path:      "/nix/store/test-orphan.drv".to_string(),
      system:        None,
      outputs:       None,
      is_aggregate:  None,
      constituents:  None,
    })
    .await
    .expect("create build");

  let _ = fc_common::repo::builds::start(&pool, build.id).await;

  // Simulate the build being stuck for a while by manually backdating
  // started_at
  // Truly a genius way to test.
  sqlx::query(
    "UPDATE builds SET started_at = NOW() - INTERVAL '10 minutes' WHERE id = \
     $1",
  )
  .bind(build.id)
  .execute(&pool)
  .await
  .expect("backdate build");

  // Reset orphaned builds (older than 5 minutes)
  let count = fc_common::repo::builds::reset_orphaned(&pool, 300)
    .await
    .expect("reset orphaned");
  assert!(count >= 1, "should have reset at least 1 orphaned build");

  // Verify build is pending again
  let reset_build = fc_common::repo::builds::get(&pool, build.id)
    .await
    .expect("get build");
  assert_eq!(reset_build.status, fc_common::models::BuildStatus::Pending);

  // Clean up
  let _ = fc_common::repo::projects::delete(&pool, project.id).await;
}
