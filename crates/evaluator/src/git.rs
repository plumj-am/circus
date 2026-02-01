use std::path::{Path, PathBuf};

use fc_common::error::Result;
use git2::Repository;

/// Clone or fetch a repository. Returns (repo_path, head_commit_hash).
pub fn clone_or_fetch(url: &str, work_dir: &Path, project_name: &str) -> Result<(PathBuf, String)> {
    let repo_path = work_dir.join(project_name);

    let repo = if repo_path.exists() {
        let repo = Repository::open(&repo_path)?;
        // Fetch origin — scope the borrow so `remote` is dropped before we move `repo`
        {
            let mut remote = repo.find_remote("origin")?;
            remote.fetch(&["refs/heads/*:refs/remotes/origin/*"], None, None)?;
        }
        repo
    } else {
        Repository::clone(url, &repo_path)?
    };

    // Get HEAD commit hash
    let head = repo.head()?;
    let commit = head.peel_to_commit()?;
    let hash = commit.id().to_string();

    Ok((repo_path, hash))
}
