use std::path::{Path, PathBuf};
use std::process::Command;

use thiserror::Error;

use crate::ExitCode;
use crate::manifest::DispatchMode;

/// A handle to the pinned workspace directory.
///
/// Every git, shell, and MCP operation MUST take its `cwd` from this handle.
/// No component may call `std::env::set_current_dir` or assume a process-global cwd.
#[derive(Debug, Clone)]
pub struct Workspace {
    path: PathBuf,
}

#[derive(Debug, Error)]
pub enum WorkspaceError {
    #[error("git subprocess failed: {cmd}\nstdout: {stdout}\nstderr: {stderr}")]
    GitFailed {
        cmd: String,
        stdout: String,
        stderr: String,
    },
    #[error("io error setting up workspace: {0}")]
    Io(#[from] std::io::Error),
}

impl WorkspaceError {
    pub fn exit_code(&self) -> ExitCode {
        ExitCode::HardAbort
    }
}

impl Workspace {
    /// The absolute path of the workspace.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Construct a Workspace handle from an existing path (for unit tests; skips git setup).
    #[cfg(test)]
    pub fn from_path_unchecked(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    /// Set up the worktree at `workspace_path` for the given ticket branch.
    ///
    /// This implements the spec's independent-dispatch behaviour:
    /// - If the worktree doesn't exist → `git worktree add` at the default-branch tip.
    /// - If a stale worktree exists → force-remove then recreate.
    /// - If the branch already exists → reset it to the default-branch tip.
    ///
    /// Note: verify-loop dispatches are a documented exception (see design.md Open Questions).
    pub fn setup(
        repo_root: &Path,
        workspace_path: &Path,
        ticket_id: &str,
        dispatch_mode: DispatchMode,
    ) -> Result<Self, WorkspaceError> {
        let branch = format!("score/{ticket_id}");

        if dispatch_mode == DispatchMode::VerifyLoop {
            return Self::setup_verify_loop(repo_root, workspace_path, &branch);
        }

        // Resolve the default remote branch (e.g., refs/remotes/origin/HEAD → origin/main).
        let base = resolve_default_branch(repo_root)?;

        // Force-remove any stale worktree at this path.
        if workspace_path.exists() {
            git(
                repo_root,
                &[
                    "worktree",
                    "remove",
                    "--force",
                    workspace_path.to_str().unwrap(),
                ],
            )?;
        }

        // Determine whether the branch already exists.
        let branch_exists = git_output(repo_root, &["rev-parse", "--verify", &branch]).is_ok();

        if branch_exists {
            // Reset the existing branch to the base tip.
            git(repo_root, &["branch", "-f", &branch, &base])?;
            git(
                repo_root,
                &["worktree", "add", workspace_path.to_str().unwrap(), &branch],
            )?;
        } else {
            // Create a new branch at the base tip.
            git(
                repo_root,
                &[
                    "worktree",
                    "add",
                    "-b",
                    &branch,
                    workspace_path.to_str().unwrap(),
                    &base,
                ],
            )?;
        }

        Ok(Self {
            path: workspace_path.to_path_buf(),
        })
    }

    fn setup_verify_loop(
        repo_root: &Path,
        workspace_path: &Path,
        branch: &str,
    ) -> Result<Self, WorkspaceError> {
        git_output(repo_root, &["rev-parse", "--verify", branch])?;

        if workspace_path.exists() {
            let current = git_output(workspace_path, &["rev-parse", "--abbrev-ref", "HEAD"])?;
            if current.trim() == branch {
                return Ok(Self {
                    path: workspace_path.to_path_buf(),
                });
            }
            git(
                repo_root,
                &[
                    "worktree",
                    "remove",
                    "--force",
                    workspace_path.to_str().unwrap(),
                ],
            )?;
        }

        git(
            repo_root,
            &["worktree", "add", workspace_path.to_str().unwrap(), branch],
        )?;

        Ok(Self {
            path: workspace_path.to_path_buf(),
        })
    }

    /// Remove the worktree (best-effort).
    pub fn remove(&self, repo_root: &Path) {
        let _ = git(
            repo_root,
            &["worktree", "remove", "--force", self.path.to_str().unwrap()],
        );
    }

    /// Run `git -C <workspace>` with the given args.
    pub fn git(&self, args: &[&str]) -> Result<String, WorkspaceError> {
        git(&self.path, args)
    }

    /// Return all files changed vs HEAD using `git diff --numstat`.
    ///
    /// Each entry has `path`, `additions`, and `deletions` per the report schema.
    pub fn files_changed(&self) -> Vec<crate::report::FileChange> {
        git_output(&self.path, &["diff", "--numstat", "HEAD"])
            .unwrap_or_default()
            .lines()
            .filter(|l| !l.is_empty())
            .filter_map(|line| {
                // Format: "additions\tdeletions\tpath"
                let mut parts = line.splitn(3, '\t');
                let additions: u32 = parts.next()?.parse().unwrap_or(0);
                let deletions: u32 = parts.next()?.parse().unwrap_or(0);
                let path = parts.next()?.to_string();
                Some(crate::report::FileChange {
                    path,
                    additions,
                    deletions,
                })
            })
            .collect()
    }
}

fn resolve_default_branch(repo_root: &Path) -> Result<String, WorkspaceError> {
    // Try origin/HEAD first (set when the remote is tracked).
    if let Ok(out) = git_output(repo_root, &["symbolic-ref", "refs/remotes/origin/HEAD"]) {
        let trimmed = out.trim();
        // Returns e.g. "refs/remotes/origin/main" — convert to "origin/main".
        if let Some(short) = trimmed.strip_prefix("refs/remotes/") {
            return Ok(short.to_string());
        }
        return Ok(trimmed.to_string());
    }
    // Fallback: use HEAD of the main checkout.
    Ok("HEAD".to_string())
}

fn git(cwd: &Path, args: &[&str]) -> Result<String, WorkspaceError> {
    let output = Command::new("git").arg("-C").arg(cwd).args(args).output()?;

    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    } else {
        Err(WorkspaceError::GitFailed {
            cmd: format!("git -C {} {}", cwd.display(), args.join(" ")),
            stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        })
    }
}

fn git_output(cwd: &Path, args: &[&str]) -> Result<String, WorkspaceError> {
    git(cwd, args)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn init_bare_repo(dir: &Path) {
        Command::new("git").arg("init").arg(dir).output().unwrap();
        Command::new("git")
            .args([
                "-C",
                dir.to_str().unwrap(),
                "config",
                "user.email",
                "test@example.com",
            ])
            .output()
            .unwrap();
        Command::new("git")
            .args(["-C", dir.to_str().unwrap(), "config", "user.name", "Test"])
            .output()
            .unwrap();
        // Create an initial commit so HEAD is valid.
        let readme = dir.join("README.md");
        fs::write(&readme, "test\n").unwrap();
        Command::new("git")
            .args(["-C", dir.to_str().unwrap(), "add", "."])
            .output()
            .unwrap();
        Command::new("git")
            .args(["-C", dir.to_str().unwrap(), "commit", "-m", "init"])
            .output()
            .unwrap();
    }

    #[test]
    fn clean_worktree_create() {
        let tmp = tempfile_dir();
        let repo = tmp.join("repo");
        let workspace_path = tmp.join("wt");
        fs::create_dir_all(&repo).unwrap();
        init_bare_repo(&repo);

        let ws = Workspace::setup(
            &repo,
            &workspace_path,
            "test-ticket",
            DispatchMode::Independent,
        )
        .unwrap();
        assert!(ws.path().exists());
        // Clean up.
        ws.remove(&repo);
    }

    #[test]
    fn stale_worktree_replaced() {
        let tmp = tempfile_dir();
        let repo = tmp.join("repo2");
        let workspace_path = tmp.join("wt2");
        fs::create_dir_all(&repo).unwrap();
        init_bare_repo(&repo);

        // Create once.
        let ws = Workspace::setup(
            &repo,
            &workspace_path,
            "test-ticket",
            DispatchMode::Independent,
        )
        .unwrap();
        assert!(ws.path().exists());

        // Create again — stale replacement.
        let ws2 = Workspace::setup(
            &repo,
            &workspace_path,
            "test-ticket",
            DispatchMode::Independent,
        )
        .unwrap();
        assert!(ws2.path().exists());
        ws2.remove(&repo);
    }

    #[test]
    fn independent_dispatch_resets_existing_branch_to_base() {
        let tmp = tempfile_dir();
        let repo = tmp.join("repo3");
        let workspace_path = tmp.join("wt3");
        fs::create_dir_all(&repo).unwrap();
        init_bare_repo(&repo);

        let ws = Workspace::setup(
            &repo,
            &workspace_path,
            "test-ticket",
            DispatchMode::Independent,
        )
        .unwrap();
        fs::write(workspace_path.join("changed.txt"), "branch tip\n").unwrap();
        ws.git(&["add", "."]).unwrap();
        ws.git(&["commit", "-m", "branch tip"]).unwrap();
        ws.remove(&repo);

        let ws2 = Workspace::setup(
            &repo,
            &workspace_path,
            "test-ticket",
            DispatchMode::Independent,
        )
        .unwrap();
        assert!(!workspace_path.join("changed.txt").exists());
        ws2.remove(&repo);
    }

    #[test]
    fn verify_loop_preserves_existing_branch_tip() {
        let tmp = tempfile_dir();
        let repo = tmp.join("repo4");
        let workspace_path = tmp.join("wt4");
        fs::create_dir_all(&repo).unwrap();
        init_bare_repo(&repo);

        let ws = Workspace::setup(
            &repo,
            &workspace_path,
            "test-ticket",
            DispatchMode::Independent,
        )
        .unwrap();
        fs::write(workspace_path.join("executor.txt"), "committed\n").unwrap();
        ws.git(&["add", "."]).unwrap();
        ws.git(&["commit", "-m", "executor work"]).unwrap();
        ws.remove(&repo);

        let ws2 = Workspace::setup(
            &repo,
            &workspace_path,
            "test-ticket",
            DispatchMode::VerifyLoop,
        )
        .unwrap();
        assert_eq!(
            fs::read_to_string(workspace_path.join("executor.txt")).unwrap(),
            "committed\n"
        );
        ws2.remove(&repo);
    }

    fn tempfile_dir() -> PathBuf {
        let base = std::env::temp_dir().join("voice-ws-tests");
        use std::time::{SystemTime, UNIX_EPOCH};
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .subsec_nanos();
        base.join(unique.to_string())
    }
}
