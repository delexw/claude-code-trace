use std::fs;
use std::path::{Path, PathBuf};

/// Returns a display name for a project directory.
pub fn project_name(cwd: &str, git_branch: &str) -> String {
    if cwd.is_empty() {
        return String::new();
    }
    let cleaned = Path::new(cwd).to_path_buf();

    if let Some(root) = find_git_repo_root(&cleaned) {
        return root
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("")
            .to_string();
    }

    let name = cleaned
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("")
        .to_string();
    trim_branch_suffix(&name, git_branch)
}

/// Returns the git toplevel for the given directory. If the directory is
/// inside a git worktree, resolves to the main working tree root.
/// Falls back to the original path if not a git repo.
pub fn resolve_git_root(dir: &str) -> String {
    if let Some(root) = find_git_repo_root(Path::new(dir)) {
        root.to_string_lossy().to_string()
    } else {
        dir.to_string()
    }
}

fn find_git_repo_root(dir: &Path) -> Option<PathBuf> {
    let mut current = if dir.is_dir() {
        dir.to_path_buf()
    } else {
        dir.parent()?.to_path_buf()
    };

    loop {
        let git_path = current.join(".git");
        if let Ok(meta) = fs::metadata(&git_path) {
            if meta.is_dir() {
                return Some(current);
            }
            if meta.is_file() {
                // Worktree: try to resolve main repo root
                if let Some(root) = repo_root_from_git_file(&current, &git_path) {
                    return Some(root);
                }
                return Some(current);
            }
        }
        if !current.pop() {
            return None;
        }
    }
}

fn repo_root_from_git_file(repo_dir: &Path, git_file: &Path) -> Option<PathBuf> {
    let content = fs::read_to_string(git_file).ok()?;
    let git_dir_str = content
        .lines()
        .find(|l| l.to_lowercase().starts_with("gitdir:"))?
        .trim_start_matches(|c: char| !c.is_ascii_whitespace() && c != ':')
        .trim_start_matches(':')
        .trim();

    let git_dir = if Path::new(git_dir_str).is_absolute() {
        PathBuf::from(git_dir_str)
    } else {
        git_file.parent()?.join(git_dir_str).canonicalize().ok()?
    };

    // Try commondir
    let common_dir_path = git_dir.join("commondir");
    if let Ok(common_content) = fs::read_to_string(&common_dir_path) {
        let common = common_content.trim();
        let common_path = if Path::new(common).is_absolute() {
            PathBuf::from(common)
        } else {
            git_dir.join(common).canonicalize().ok()?
        };
        if common_path.file_name().and_then(|n| n.to_str()) == Some(".git") {
            return common_path.parent().map(|p| p.to_path_buf());
        }
    }

    // Fallback: parse worktrees path
    let git_dir_str = git_dir.to_string_lossy();
    let marker = format!("{}worktrees{}", std::path::MAIN_SEPARATOR, std::path::MAIN_SEPARATOR);
    if let Some(idx) = git_dir_str.find(&format!(".git{}", marker)) {
        let root = &git_dir_str[..idx];
        if !root.is_empty() {
            return Some(PathBuf::from(root.trim_end_matches(std::path::MAIN_SEPARATOR)));
        }
    }

    Some(repo_dir.to_path_buf())
}

fn trim_branch_suffix(name: &str, git_branch: &str) -> String {
    let branch = git_branch.trim().trim_start_matches("refs/heads/");
    if name.is_empty() || branch.is_empty() {
        return name.to_string();
    }
    let branch_token = normalize_branch_token(branch);
    if branch_token.is_empty() || is_default_branch(&branch_token) {
        return name.to_string();
    }

    for sep in &["-", "_"] {
        let suffix = format!("{}{}", sep, branch_token);
        if name.to_lowercase().ends_with(&suffix.to_lowercase()) {
            let base = name[..name.len() - suffix.len()].trim_end_matches(&['-', '_'][..]);
            if !base.is_empty() {
                return base.to_string();
            }
        }
    }
    name.to_string()
}

fn normalize_branch_token(branch: &str) -> String {
    let mut result = String::with_capacity(branch.len());
    let mut last_dash = false;
    for c in branch.chars() {
        if c.is_alphanumeric() {
            result.push(c.to_lowercase().next().unwrap_or(c));
            last_dash = false;
        } else if !last_dash {
            result.push('-');
            last_dash = true;
        }
    }
    result.trim_matches('-').to_string()
}

fn is_default_branch(branch: &str) -> bool {
    matches!(
        branch.to_lowercase().as_str(),
        "main" | "master" | "trunk" | "develop" | "dev"
    )
}
