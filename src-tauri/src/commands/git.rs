use serde::Serialize;
use std::process::Command;

/// GitInfo holds git repository state for the frontend.
#[derive(Debug, Clone, Serialize)]
pub struct GitInfo {
    pub branch: String,
    pub dirty: bool,
    pub worktree_dirs: Vec<String>,
}

/// Get git info (branch, dirty state, worktree dirs) for a directory.
#[tauri::command]
pub fn get_git_info(cwd: String) -> GitInfo {
    if cwd.is_empty() {
        return GitInfo {
            branch: String::new(),
            dirty: false,
            worktree_dirs: Vec::new(),
        };
    }

    let branch = check_git_branch(&cwd);
    let dirty = check_git_dirty(&cwd);
    let worktree_dirs = discover_worktree_dirs(&cwd);

    GitInfo {
        branch,
        dirty,
        worktree_dirs,
    }
}

fn check_git_branch(cwd: &str) -> String {
    Command::new("git")
        .args(["-C", cwd, "rev-parse", "--abbrev-ref", "HEAD"])
        .output()
        .ok()
        .and_then(|o| {
            if o.status.success() {
                String::from_utf8(o.stdout).ok().map(|s| s.trim().to_string())
            } else {
                None
            }
        })
        .unwrap_or_default()
}

fn check_git_dirty(cwd: &str) -> bool {
    Command::new("git")
        .args(["-C", cwd, "status", "--porcelain"])
        .output()
        .ok()
        .map(|o| o.status.success() && !o.stdout.is_empty())
        .unwrap_or(false)
}

fn discover_worktree_dirs(cwd: &str) -> Vec<String> {
    Command::new("git")
        .args(["-C", cwd, "worktree", "list", "--porcelain"])
        .output()
        .ok()
        .and_then(|o| {
            if o.status.success() {
                String::from_utf8(o.stdout).ok().map(|output| {
                    parse_worktree_paths(&output)
                })
            } else {
                None
            }
        })
        .unwrap_or_default()
}

fn parse_worktree_paths(output: &str) -> Vec<String> {
    output
        .lines()
        .filter_map(|line| {
            line.strip_prefix("worktree ").map(|p| p.to_string())
        })
        .collect()
}
