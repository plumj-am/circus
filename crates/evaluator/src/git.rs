use std::path::{Path, PathBuf};

use fc_common::error::Result;
use git2::Repository;

/// Clone or fetch a repository. Returns (`repo_path`, `commit_hash`).
///
/// If `branch` is `Some`, resolve `refs/remotes/origin/<branch>` instead of
/// HEAD.
#[tracing::instrument(skip(work_dir))]
pub fn clone_or_fetch(
  url: &str,
  work_dir: &Path,
  project_name: &str,
  branch: Option<&str>,
) -> Result<(PathBuf, String)> {
  let repo_path = work_dir.join(project_name);

  let repo = if repo_path.exists() {
    let repo = Repository::open(&repo_path)?;
    // Fetch origin — scope the borrow so `remote` is dropped before we move
    // `repo`
    {
      let mut remote = repo.find_remote("origin")?;
      remote.fetch(&["refs/heads/*:refs/remotes/origin/*"], None, None)?;
    }
    repo
  } else {
    Repository::clone(url, &repo_path)?
  };

  // Resolve commit: use specific branch ref or fall back to HEAD
  let hash = if let Some(branch_name) = branch {
    let refname = format!("refs/remotes/origin/{branch_name}");
    let reference = repo.find_reference(&refname).map_err(|e| {
      fc_common::error::CiError::NotFound(format!(
        "Branch '{branch_name}' not found ({refname}): {e}"
      ))
    })?;
    let commit = reference.peel_to_commit()?;
    commit.id().to_string()
  } else {
    let head = repo.head()?;
    let commit = head.peel_to_commit()?;
    commit.id().to_string()
  };

  Ok((repo_path, hash))
}
