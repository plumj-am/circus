use std::path::{Path, PathBuf};

use circus_common::error::{CiError, Result};
use git2::Repository;

/// Refspecs fetched on every sync. The first is the standard branch fetch.
/// The two remaining refspecs make pull-request / merge-request commits
/// reachable so the evaluator can check them out when a webhook pushes
/// an evaluation for a PR head commit:
///
///   - `refs/pull/*/head` is GitHub, Gitea, and Forgejo's PR ref.
///   - `refs/merge-requests/*/head` is GitLab's MR ref.
///
/// Forges that don't publish these refs (Cgit, plain Git remotes) will
/// fail the fetch on these refspecs; we treat that as non-fatal.
const FETCH_REFSPECS_REQUIRED: &[&str] =
  &["refs/heads/*:refs/remotes/origin/*"];
const FETCH_REFSPECS_OPTIONAL: &[&str] = &[
  "refs/pull/*/head:refs/remotes/origin/pr/*",
  "refs/merge-requests/*/head:refs/remotes/origin/mr/*",
];

fn fetch_all_refs(repo: &Repository) -> Result<()> {
  let mut remote = repo.find_remote("origin")?;
  remote.fetch(FETCH_REFSPECS_REQUIRED, None, None)?;
  // PR/MR refspecs are forge-specific; ignore failures so a plain Git
  // remote without pull refs still evaluates.
  for spec in FETCH_REFSPECS_OPTIONAL {
    if let Err(e) = remote.fetch(&[*spec], None, None) {
      tracing::debug!(refspec = spec, "Optional fetch failed: {e}");
    }
  }
  Ok(())
}

/// Clone or fetch a repository. Returns (`repo_path`, `commit_hash`).
///
/// If `branch` is `Some`, resolve `refs/remotes/origin/<branch>` instead of
/// HEAD.
///
/// # Errors
///
/// Returns error if git operations fail.
#[tracing::instrument(skip(work_dir))]
pub fn clone_or_fetch(
  url: &str,
  work_dir: &Path,
  project_name: &str,
  branch: Option<&str>,
) -> Result<(PathBuf, String)> {
  let repo_path = work_dir.join(project_name);

  let is_fetch = repo_path.exists();

  let repo = if is_fetch {
    let repo = Repository::open(&repo_path)?;
    fetch_all_refs(&repo)?;
    repo
  } else {
    let repo = Repository::clone(url, &repo_path)?;
    // Fresh clone only brought branch refs; pull PR/MR refs as well so
    // the freshly-cloned repo has the same coverage as a re-fetched one.
    for spec in FETCH_REFSPECS_OPTIONAL {
      if let Err(e) = repo
        .find_remote("origin")
        .and_then(|mut r| r.fetch(&[*spec], None, None))
      {
        tracing::debug!(refspec = spec, "Optional fetch failed: {e}");
      }
    }
    repo
  };

  // Resolve commit from remote refs (which are always up-to-date after fetch).
  // When no branch is specified, detect the default branch from local HEAD's
  // tracking target.
  let branch_name = if let Some(b) = branch {
    b.to_string()
  } else {
    let head = repo.head()?;
    head.shorthand().unwrap_or("master").to_string()
  };

  let remote_ref = format!("refs/remotes/origin/{branch_name}");
  let reference = repo.find_reference(&remote_ref).map_err(|e| {
    CiError::NotFound(format!(
      "Branch '{branch_name}' not found ({remote_ref}): {e}"
    ))
  })?;
  let commit = reference.peel_to_commit()?;
  let hash = commit.id().to_string();

  // After fetch, update the working tree so nix evaluation sees the latest
  // files. Skip on fresh clone since the checkout is already current.
  if is_fetch {
    repo.checkout_tree(
      commit.as_object(),
      Some(git2::build::CheckoutBuilder::new().force()),
    )?;
    repo.set_head_detached(commit.id())?;
  }

  Ok((repo_path, hash))
}

/// Fetch from origin and check out a specific commit SHA. Used to
/// evaluate a pushed PR head commit that may not be a branch tip.
///
/// The repo must already exist (callers invoke `clone_or_fetch` first to
/// establish the working tree). After this returns, `repo_path` has the
/// requested commit checked out and is ready for nix evaluation.
///
/// # Errors
///
/// Returns `NotFound` if the SHA is not reachable from any fetched ref.
#[tracing::instrument(skip(work_dir))]
pub fn fetch_and_checkout_commit(
  url: &str,
  work_dir: &Path,
  project_name: &str,
  commit_sha: &str,
) -> Result<PathBuf> {
  let repo_path = work_dir.join(project_name);

  let repo = if repo_path.exists() {
    Repository::open(&repo_path)?
  } else {
    Repository::clone(url, &repo_path)?
  };

  fetch_all_refs(&repo)?;

  let oid = git2::Oid::from_str(commit_sha).map_err(|e| {
    CiError::Validation(format!("Invalid commit SHA '{commit_sha}': {e}"))
  })?;

  let commit = repo.find_commit(oid).map_err(|e| {
    CiError::NotFound(format!(
      "Commit {commit_sha} not reachable on origin (fetched branches and \
       pull/merge-request refs): {e}"
    ))
  })?;

  repo.checkout_tree(
    commit.as_object(),
    Some(git2::build::CheckoutBuilder::new().force()),
  )?;
  repo.set_head_detached(commit.id())?;

  Ok(repo_path)
}
