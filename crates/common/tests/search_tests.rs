//! Integration tests for advanced search functionality
//! Requires TEST_DATABASE_URL to be set to a PostgreSQL connection string.

use fc_common::{models::*, repo, repo::search::*};
use uuid::Uuid;

async fn get_pool() -> Option<sqlx::PgPool> {
  let url = match std::env::var("TEST_DATABASE_URL") {
    Ok(url) => url,
    Err(_) => {
      println!("Skipping search test: TEST_DATABASE_URL not set");
      return None;
    },
  };

  let pool = sqlx::postgres::PgPoolOptions::new()
    .max_connections(5)
    .connect(&url)
    .await
    .ok()?;

  // Run migrations
  sqlx::migrate!("./migrations").run(&pool).await.ok()?;

  Some(pool)
}

#[tokio::test]
async fn test_project_search() {
  let pool = match get_pool().await {
    Some(p) => p,
    None => return,
  };

  // Create test projects
  let project1 = repo::projects::create(&pool, CreateProject {
    name:           format!("search-test-alpha-{}", Uuid::new_v4().simple()),
    description:    Some("Alpha testing project".to_string()),
    repository_url: "https://github.com/test/alpha".to_string(),
  })
  .await
  .expect("create project 1");

  let project2 = repo::projects::create(&pool, CreateProject {
    name:           format!("search-test-beta-{}", Uuid::new_v4().simple()),
    description:    Some("Beta testing project".to_string()),
    repository_url: "https://github.com/test/beta".to_string(),
  })
  .await
  .expect("create project 2");

  // Search for "alpha"
  let params = SearchParams {
    query:              "alpha".to_string(),
    entities:           vec![SearchEntity::Projects],
    limit:              10,
    offset:             0,
    build_filters:      None,
    project_filters:    None,
    jobset_filters:     None,
    evaluation_filters: None,
    build_sort:         None,
    project_sort:       None,
  };

  let results = search(&pool, &params).await.expect("search");
  assert_eq!(results.projects.len(), 1);
  assert_eq!(results.projects[0].id, project1.id);
  assert_eq!(results.total_projects, 1);

  // Search for "testing" (should match both)
  let params = SearchParams {
    query:              "testing".to_string(),
    entities:           vec![SearchEntity::Projects],
    limit:              10,
    offset:             0,
    build_filters:      None,
    project_filters:    None,
    jobset_filters:     None,
    evaluation_filters: None,
    build_sort:         None,
    project_sort:       None,
  };

  let results = search(&pool, &params).await.expect("search");
  assert_eq!(results.projects.len(), 2);
  assert_eq!(results.total_projects, 2);

  // Cleanup
  repo::projects::delete(&pool, project1.id).await.ok();
  repo::projects::delete(&pool, project2.id).await.ok();
}

#[tokio::test]
async fn test_build_search_with_filters() {
  let pool = match get_pool().await {
    Some(p) => p,
    None => return,
  };

  // Setup
  let project = repo::projects::create(&pool, CreateProject {
    name:           format!("build-search-{}", Uuid::new_v4().simple()),
    description:    None,
    repository_url: "https://github.com/test/repo".to_string(),
  })
  .await
  .expect("create project");

  let jobset = repo::jobsets::create(&pool, CreateJobset {
    project_id:        project.id,
    name:              "default".to_string(),
    nix_expression:    "packages".to_string(),
    enabled:           Some(true),
    flake_mode:        None,
    check_interval:    None,
    branch:            None,
    scheduling_shares: None,
  })
  .await
  .expect("create jobset");

  // Create builds with different statuses
  let build1 = repo::builds::create(&pool, &CreateBuild {
    project_id:    project.id,
    jobset_id:     jobset.id,
    evaluation_id: None,
    job_name:      "package-hello".to_string(),
    drv_path:      "/nix/store/...-hello.drv".to_string(),
    priority:      100,
  })
  .await
  .expect("create build 1");

  // Update build status
  repo::builds::update_status(&pool, build1.id, "succeeded")
    .await
    .expect("update build 1 status");

  let build2 = repo::builds::create(&pool, &CreateBuild {
    project_id:    project.id,
    jobset_id:     jobset.id,
    evaluation_id: None,
    job_name:      "package-world".to_string(),
    drv_path:      "/nix/store/...-world.drv".to_string(),
    priority:      50,
  })
  .await
  .expect("create build 2");

  repo::builds::update_status(&pool, build2.id, "failed")
    .await
    .expect("update build 2 status");

  // Search by job name
  let params = SearchParams {
    query:              "hello".to_string(),
    entities:           vec![SearchEntity::Builds],
    limit:              10,
    offset:             0,
    build_filters:      None,
    project_filters:    None,
    jobset_filters:     None,
    evaluation_filters: None,
    build_sort:         None,
    project_sort:       None,
  };

  let results = search(&pool, &params).await.expect("search");
  assert_eq!(results.builds.len(), 1);
  assert_eq!(results.builds[0].id, build1.id);

  // Search with status filter (succeeded)
  let params = SearchParams {
    query:              "".to_string(),
    entities:           vec![SearchEntity::Builds],
    limit:              10,
    offset:             0,
    build_filters:      Some(BuildSearchFilters {
      status:          Some(BuildStatusFilter::Succeeded),
      project_id:      None,
      jobset_id:       None,
      evaluation_id:   None,
      created_after:   None,
      created_before:  None,
      min_priority:    None,
      max_priority:    None,
      has_substitutes: None,
    }),
    project_filters:    None,
    jobset_filters:     None,
    evaluation_filters: None,
    build_sort:         None,
    project_sort:       None,
  };

  let results = search(&pool, &params).await.expect("search");
  assert_eq!(results.builds.len(), 1);
  assert_eq!(results.builds[0].id, build1.id);

  // Search with priority filter
  let params = SearchParams {
    query:              "".to_string(),
    entities:           vec![SearchEntity::Builds],
    limit:              10,
    offset:             0,
    build_filters:      Some(BuildSearchFilters {
      status:          None,
      project_id:      None,
      jobset_id:       None,
      evaluation_id:   None,
      created_after:   None,
      created_before:  None,
      min_priority:    Some(75),
      max_priority:    None,
      has_substitutes: None,
    }),
    project_filters:    None,
    jobset_filters:     None,
    evaluation_filters: None,
    build_sort:         None,
    project_sort:       None,
  };

  let results = search(&pool, &params).await.expect("search");
  assert_eq!(results.builds.len(), 1);
  assert_eq!(results.builds[0].id, build1.id);

  // Cleanup
  repo::jobsets::delete(&pool, jobset.id).await.ok();
  repo::projects::delete(&pool, project.id).await.ok();
}

#[tokio::test]
async fn test_multi_entity_search() {
  let pool = match get_pool().await {
    Some(p) => p,
    None => return,
  };

  // Create project with jobset and builds
  let project = repo::projects::create(&pool, CreateProject {
    name:           format!("multi-search-{}", Uuid::new_v4().simple()),
    description:    Some("Multi-entity search test".to_string()),
    repository_url: "https://github.com/test/multi".to_string(),
  })
  .await
  .expect("create project");

  let jobset = repo::jobsets::create(&pool, CreateJobset {
    project_id:        project.id,
    name:              "default".to_string(),
    nix_expression:    "packages".to_string(),
    enabled:           Some(true),
    flake_mode:        None,
    check_interval:    None,
    branch:            None,
    scheduling_shares: None,
  })
  .await
  .expect("create jobset");

  let build = repo::builds::create(&pool, &CreateBuild {
    project_id:    project.id,
    jobset_id:     jobset.id,
    evaluation_id: None,
    job_name:      "test-job".to_string(),
    drv_path:      "/nix/store/...-test.drv".to_string(),
    priority:      100,
  })
  .await
  .expect("create build");

  // Search across all entities
  let params = SearchParams {
    query:              "test".to_string(),
    entities:           vec![
      SearchEntity::Projects,
      SearchEntity::Jobsets,
      SearchEntity::Builds,
    ],
    limit:              10,
    offset:             0,
    build_filters:      None,
    project_filters:    None,
    jobset_filters:     None,
    evaluation_filters: None,
    build_sort:         None,
    project_sort:       None,
  };

  let results = search(&pool, &params).await.expect("search");
  assert!(!results.projects.is_empty());
  assert!(!results.jobsets.is_empty());
  assert!(!results.builds.is_empty());

  // Cleanup
  repo::builds::delete(&pool, build.id).await.ok();
  repo::jobsets::delete(&pool, jobset.id).await.ok();
  repo::projects::delete(&pool, project.id).await.ok();
}

#[tokio::test]
async fn test_search_pagination() {
  let pool = match get_pool().await {
    Some(p) => p,
    None => return,
  };

  // Create multiple projects
  let mut project_ids = vec![];
  for i in 0..5 {
    let project = repo::projects::create(&pool, CreateProject {
      name:           format!("page-test-{}-{}", i, Uuid::new_v4().simple()),
      description:    Some(format!("Page test project {}", i)),
      repository_url: "https://github.com/test/page".to_string(),
    })
    .await
    .expect("create project");
    project_ids.push(project.id);
  }

  // Search with limit 2
  let params = SearchParams {
    query:              "page-test".to_string(),
    entities:           vec![SearchEntity::Projects],
    limit:              2,
    offset:             0,
    build_filters:      None,
    project_filters:    None,
    jobset_filters:     None,
    evaluation_filters: None,
    build_sort:         None,
    project_sort:       None,
  };

  let results = search(&pool, &params).await.expect("search");
  assert_eq!(results.projects.len(), 2);
  assert!(results.total_projects >= 5);

  // Search with offset 2
  let params = SearchParams {
    query:              "page-test".to_string(),
    entities:           vec![SearchEntity::Projects],
    limit:              2,
    offset:             2,
    build_filters:      None,
    project_filters:    None,
    jobset_filters:     None,
    evaluation_filters: None,
    build_sort:         None,
    project_sort:       None,
  };

  let results = search(&pool, &params).await.expect("search");
  assert_eq!(results.projects.len(), 2);

  // Cleanup
  for id in project_ids {
    repo::projects::delete(&pool, id).await.ok();
  }
}

#[tokio::test]
async fn test_search_sorting() {
  let pool = match get_pool().await {
    Some(p) => p,
    None => return,
  };

  // Create projects in reverse alphabetical order
  let project_z = repo::projects::create(&pool, CreateProject {
    name:           format!("zzz-sort-test-{}", Uuid::new_v4().simple()),
    description:    None,
    repository_url: "https://github.com/test/z".to_string(),
  })
  .await
  .expect("create project z");

  let project_a = repo::projects::create(&pool, CreateProject {
    name:           format!("aaa-sort-test-{}", Uuid::new_v4().simple()),
    description:    None,
    repository_url: "https://github.com/test/a".to_string(),
  })
  .await
  .expect("create project a");

  // Search sorted by name ascending
  let params = SearchParams {
    query:              "sort-test".to_string(),
    entities:           vec![SearchEntity::Projects],
    limit:              10,
    offset:             0,
    build_filters:      None,
    project_filters:    None,
    jobset_filters:     None,
    evaluation_filters: None,
    build_sort:         None,
    project_sort:       Some((ProjectSortField::Name, SortOrder::Asc)),
  };

  let results = search(&pool, &params).await.expect("search");
  assert_eq!(results.projects.len(), 2);
  assert!(results.projects[0].name.starts_with("aaa"));
  assert!(results.projects[1].name.starts_with("zzz"));

  // Cleanup
  repo::projects::delete(&pool, project_a.id).await.ok();
  repo::projects::delete(&pool, project_z.id).await.ok();
}

#[tokio::test]
async fn test_empty_search() {
  let pool = match get_pool().await {
    Some(p) => p,
    None => return,
  };

  // Empty query should return all entities (up to limit)
  let params = SearchParams {
    query:              "".to_string(),
    entities:           vec![SearchEntity::Projects],
    limit:              10,
    offset:             0,
    build_filters:      None,
    project_filters:    None,
    jobset_filters:     None,
    evaluation_filters: None,
    build_sort:         None,
    project_sort:       None,
  };

  let results = search(&pool, &params).await.expect("search");
  // Should not error, just return results
  assert!(results.total_projects >= 0);
}

#[tokio::test]
async fn test_quick_search() {
  let pool = match get_pool().await {
    Some(p) => p,
    None => return,
  };

  // Create test data
  let project = repo::projects::create(&pool, CreateProject {
    name:           format!("quick-search-{}", Uuid::new_v4().simple()),
    description:    Some("Quick search test".to_string()),
    repository_url: "https://github.com/test/quick".to_string(),
  })
  .await
  .expect("create project");

  let jobset = repo::jobsets::create(&pool, CreateJobset {
    project_id:        project.id,
    name:              "default".to_string(),
    nix_expression:    "packages".to_string(),
    enabled:           Some(true),
    flake_mode:        None,
    check_interval:    None,
    branch:            None,
    scheduling_shares: None,
  })
  .await
  .expect("create jobset");

  let build = repo::builds::create(&pool, &CreateBuild {
    project_id:    project.id,
    jobset_id:     jobset.id,
    evaluation_id: None,
    job_name:      "quick-job".to_string(),
    drv_path:      "/nix/store/...-quick.drv".to_string(),
    priority:      100,
  })
  .await
  .expect("create build");

  // Quick search
  let (projects, builds) = quick_search(&pool, "quick", 10)
    .await
    .expect("quick search");
  assert!(!projects.is_empty());
  assert!(!builds.is_empty());

  // Cleanup
  repo::builds::delete(&pool, build.id).await.ok();
  repo::jobsets::delete(&pool, jobset.id).await.ok();
  repo::projects::delete(&pool, project.id).await.ok();
}
