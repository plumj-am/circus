//! Integration tests for API endpoints.
//! Requires TEST_DATABASE_URL to be set.

use axum::body::Body;
use axum::http::{Request, StatusCode};
use tower::ServiceExt;

async fn get_pool() -> Option<sqlx::PgPool> {
    let url = match std::env::var("TEST_DATABASE_URL") {
        Ok(url) => url,
        Err(_) => {
            println!("Skipping API test: TEST_DATABASE_URL not set");
            return None;
        }
    };

    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(5)
        .connect(&url)
        .await
        .ok()?;

    sqlx::migrate!("../common/migrations")
        .run(&pool)
        .await
        .ok()?;

    Some(pool)
}

fn build_app(pool: sqlx::PgPool) -> axum::Router {
    let config = fc_common::config::Config::default();
    let server_config = config.server.clone();
    let state = fc_server::state::AppState {
        pool,
        config,
        sessions: std::sync::Arc::new(dashmap::DashMap::new()),
    };
    fc_server::routes::router(state, &server_config)
}

fn build_app_with_config(pool: sqlx::PgPool, config: fc_common::config::Config) -> axum::Router {
    let server_config = config.server.clone();
    let state = fc_server::state::AppState {
        pool,
        config,
        sessions: std::sync::Arc::new(dashmap::DashMap::new()),
    };
    fc_server::routes::router(state, &server_config)
}

// ---- Existing tests ----

#[tokio::test]
async fn test_health_endpoint() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };

    let app = build_app(pool);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/health")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["status"], "ok");
    assert_eq!(json["database"], true);
}

#[tokio::test]
async fn test_project_endpoints() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };

    let app = build_app(pool);

    // Create project
    let create_body = serde_json::json!({
        "name": format!("api-test-{}", uuid::Uuid::new_v4()),
        "repository_url": "https://github.com/test/repo",
        "description": "Test project"
    });

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/projects")
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_vec(&create_body).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let project: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let project_id = project["id"].as_str().unwrap();

    // Get project
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/api/v1/projects/{project_id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    // List projects
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v1/projects")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    // Get non-existent project -> 404
    let fake_id = uuid::Uuid::new_v4();
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/api/v1/projects/{fake_id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);

    // Delete project
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri(format!("/api/v1/projects/{project_id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_builds_endpoints() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };

    let app = build_app(pool);

    // Stats endpoint
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v1/builds/stats")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    // Recent endpoint
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v1/builds/recent")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
}

// ---- Hardening tests ----

#[tokio::test]
async fn test_error_response_includes_error_code() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };

    let app = build_app(pool);
    let fake_id = uuid::Uuid::new_v4();

    let response = app
        .oneshot(
            Request::builder()
                .uri(format!("/api/v1/projects/{fake_id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(json["error_code"], "NOT_FOUND");
    assert!(json["error"].as_str().is_some());
}

#[tokio::test]
async fn test_cache_invalid_hash_returns_404() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };

    let mut config = fc_common::config::Config::default();
    config.cache.enabled = true;
    let app = build_app_with_config(pool, config);

    // Too short
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/nix-cache/tooshort.narinfo")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);

    // Contains uppercase
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/nix-cache/ABCDEFGHIJKLMNOPQRSTUVWXYZABCDEF.narinfo")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);

    // Contains special chars
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/nix-cache/abcdefghijklmnop!@#$%^&*()abcde.narinfo")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);

    // SQL injection attempt
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/nix-cache/'%20OR%201=1;%20DROP%20TABLE%20builds;--.narinfo")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);

    // Valid hash format but no matching product -> 404 (not error)
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/nix-cache/abcdefghijklmnopqrstuvwxyz012345.narinfo")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_cache_nar_invalid_hash_returns_404() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };

    let mut config = fc_common::config::Config::default();
    config.cache.enabled = true;
    let app = build_app_with_config(pool, config);

    // Invalid hash in NAR endpoint
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/nix-cache/nar/INVALID_HASH.nar.zst")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);

    // Invalid hash in uncompressed NAR endpoint
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/nix-cache/nar/INVALID_HASH.nar")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_cache_disabled_returns_404() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };

    let mut config = fc_common::config::Config::default();
    config.cache.enabled = false;
    let app = build_app_with_config(pool, config);

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/nix-cache/nix-cache-info")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/nix-cache/abcdefghijklmnopqrstuvwxyz012345.narinfo")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_search_rejects_long_query() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };

    let app = build_app(pool);

    // Query over 256 chars should return empty results
    let long_query = "a".repeat(300);
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/api/v1/search?q={long_query}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["projects"], serde_json::json!([]));
    assert_eq!(json["builds"], serde_json::json!([]));
}

#[tokio::test]
async fn test_search_rejects_empty_query() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };

    let app = build_app(pool);

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v1/search?q=")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["projects"], serde_json::json!([]));
    assert_eq!(json["builds"], serde_json::json!([]));
}

#[tokio::test]
async fn test_search_whitespace_only_query() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };

    let app = build_app(pool);

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v1/search?q=%20%20%20")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["projects"], serde_json::json!([]));
}

#[tokio::test]
async fn test_builds_list_with_system_filter() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };

    let app = build_app(pool);

    // Filter by system - should return 200 even with no results
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v1/builds?system=x86_64-linux")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert!(json["items"].is_array());
    assert!(json["total"].is_number());
}

#[tokio::test]
async fn test_builds_list_with_job_name_filter() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };

    let app = build_app(pool);

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v1/builds?job_name=hello")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert!(json["items"].is_array());
}

#[tokio::test]
async fn test_builds_list_combined_filters() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };

    let app = build_app(pool);

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v1/builds?system=aarch64-linux&status=pending&job_name=foo")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_cache_info_returns_correct_headers() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };

    let mut config = fc_common::config::Config::default();
    config.cache.enabled = true;
    let app = build_app_with_config(pool, config);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/nix-cache/nix-cache-info")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response.headers().get("content-type").unwrap(),
        "text/plain"
    );

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let body_str = String::from_utf8(body.to_vec()).unwrap();
    assert!(body_str.contains("StoreDir: /nix/store"));
    assert!(body_str.contains("WantMassQuery: 1"));
    assert!(body_str.contains("Priority: 30"));
}

#[tokio::test]
async fn test_metrics_endpoint() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };

    let app = build_app(pool);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/metrics")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    assert!(
        response
            .headers()
            .get("content-type")
            .unwrap()
            .to_str()
            .unwrap()
            .contains("text/plain")
    );

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let body_str = String::from_utf8(body.to_vec()).unwrap();
    assert!(body_str.contains("fc_builds_total"));
    assert!(body_str.contains("fc_projects_total"));
    assert!(body_str.contains("fc_evaluations_total"));
}

#[tokio::test]
async fn test_get_nonexistent_build_returns_error_code() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };

    let app = build_app(pool);
    let fake_id = uuid::Uuid::new_v4();

    let response = app
        .oneshot(
            Request::builder()
                .uri(format!("/api/v1/builds/{fake_id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["error_code"], "NOT_FOUND");
    assert!(json["error"].as_str().unwrap().contains("not found"));
}

// ---- Validation tests ----

#[tokio::test]
async fn test_create_project_validation_rejects_invalid_name() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };

    let app = build_app(pool);

    // Name starting with dash
    let body = serde_json::json!({
        "name": "-bad-name",
        "repository_url": "https://github.com/test/repo"
    });

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/projects")
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_vec(&body).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["error_code"], "VALIDATION_ERROR");
}

#[tokio::test]
async fn test_create_project_validation_rejects_bad_url() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };

    let app = build_app(pool);

    let body = serde_json::json!({
        "name": "valid-name",
        "repository_url": "ftp://bad-protocol.com/repo"
    });

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/projects")
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_vec(&body).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["error_code"], "VALIDATION_ERROR");
}

#[tokio::test]
async fn test_create_project_validation_accepts_valid() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };

    let app = build_app(pool);

    let body = serde_json::json!({
        "name": format!("valid-project-{}", uuid::Uuid::new_v4()),
        "repository_url": "https://github.com/test/repo",
        "description": "A valid project"
    });

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/projects")
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_vec(&body).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
}
