use serde::Serialize;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct CodexRoot {
    pub path: String,
    pub source: String,
    pub exists: bool,
}

fn root(path: PathBuf, source: &str) -> CodexRoot {
    CodexRoot {
        exists: path.is_dir(),
        path: path.to_string_lossy().into_owned(),
        source: source.to_owned(),
    }
}

/// A custom home replaces auto-discovery, just like other optional sources.
pub fn root_candidates(
    custom: Option<&str>,
    env_home: Option<&str>,
    home: Option<&Path>,
) -> Vec<CodexRoot> {
    if let Some(custom) = custom.map(str::trim).filter(|s| !s.is_empty()) {
        return vec![root(PathBuf::from(custom), "custom")];
    }
    let local = env_home
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(PathBuf::from)
        .or_else(|| home.map(|p| p.join(".codex")));
    local.map(|p| vec![root(p, "local")]).unwrap_or_default()
}

pub fn resolve_roots(custom: Option<&str>) -> Vec<CodexRoot> {
    let mut roots = root_candidates(
        custom,
        std::env::var("CODEX_HOME").ok().as_deref(),
        dirs::home_dir().as_deref(),
    );
    if custom.map_or(true, |s| s.trim().is_empty()) {
        roots.extend(wsl_roots());
    }
    roots
}

const NOT_A_ROLLOUT: &str =
    "path must be a Codex rollout (rollout-*.jsonl) inside a configured Codex home";

/// Resolve a caller-supplied path to its real location and accept it only when
/// it is a `rollout-*.jsonl` file under the `sessions` or `archived_sessions`
/// folder of one of `roots`. Symlinks and `..` parts are followed first, so
/// they cannot be used to step outside a home.
///
/// This is the same rule the folder scan applies, so every listed chat can be
/// opened and nothing else can. A missing file and a rejected file get the same
/// error, so the check cannot be used to probe which paths exist.
pub fn authorize_rollout(roots: &[CodexRoot], path: &str) -> Result<PathBuf, String> {
    let real = Path::new(path)
        .canonicalize()
        .map_err(|_| NOT_A_ROLLOUT.to_owned())?;
    if !real.is_file() || !super::reader::is_rollout_file_name(&real) {
        return Err(NOT_A_ROLLOUT.into());
    }
    let inside_a_home = roots.iter().filter(|root| root.exists).any(|root| {
        ["sessions", "archived_sessions"].iter().any(|subdir| {
            Path::new(&root.path)
                .join(subdir)
                .canonicalize()
                .is_ok_and(|dir| real.starts_with(dir))
        })
    });
    if inside_a_home {
        Ok(real)
    } else {
        Err(NOT_A_ROLLOUT.into())
    }
}

#[cfg(not(target_os = "windows"))]
fn wsl_roots() -> Vec<CodexRoot> {
    Vec::new()
}

#[cfg(target_os = "windows")]
fn wsl_roots() -> Vec<CodexRoot> {
    use std::sync::{Mutex, OnceLock};
    use std::time::{Duration, Instant};
    type Cache = Option<(Instant, Vec<CodexRoot>)>;
    static CACHE: OnceLock<Mutex<Cache>> = OnceLock::new();
    let mut guard = CACHE
        .get_or_init(|| Mutex::new(None))
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    if let Some((time, roots)) = &*guard {
        if time.elapsed() < Duration::from_secs(30) {
            return roots.clone();
        }
    }
    let roots = crate::wsl::list_distros()
        .into_iter()
        .filter_map(|distro| {
            use std::os::windows::process::CommandExt;
            let output = std::process::Command::new("wsl.exe")
                .args([
                    "-d",
                    &distro,
                    "--",
                    "sh",
                    "-lc",
                    "printf '%s' \"${CODEX_HOME:-$HOME/.codex}\"",
                ])
                .creation_flags(0x0800_0000)
                .output()
                .ok()?;
            if !output.status.success() {
                return None;
            }
            let home = String::from_utf8(output.stdout).ok()?;
            let path = unc_home(&distro, home.trim())?;
            Some(root(PathBuf::from(path), &format!("WSL: {distro}")))
        })
        .collect::<Vec<_>>();
    *guard = Some((Instant::now(), roots.clone()));
    roots
}

fn unc_home(distro: &str, posix_path: &str) -> Option<String> {
    if !posix_path.starts_with('/') {
        return None;
    }
    Some(format!(
        r"\\wsl.localhost\{}\{}",
        distro,
        posix_path.trim_matches('/').replace('/', "\\")
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn custom_replaces_environment_and_home() {
        let roots = root_candidates(Some("/custom"), Some("/env"), Some(Path::new("/home/demo")));
        assert_eq!(roots.len(), 1);
        assert_eq!(roots[0].path, "/custom");
        assert_eq!(roots[0].source, "custom");
    }
    #[test]
    fn environment_is_a_codex_home_not_a_parent() {
        assert_eq!(root_candidates(None, Some("/env"), None)[0].path, "/env");
        assert_eq!(
            PathBuf::from(&root_candidates(None, Some("  "), Some(Path::new("/demo")))[0].path),
            Path::new("/demo").join(".codex")
        );
        assert!(root_candidates(None, None, None).is_empty());
    }
    fn home_with(files: &[&str]) -> (tempfile::TempDir, Vec<CodexRoot>) {
        let dir = tempfile::tempdir().unwrap();
        for file in files {
            let path = dir.path().join(file);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, "{}\n").unwrap();
        }
        let roots = root_candidates(dir.path().to_str(), None, None);
        (dir, roots)
    }

    fn rejected(roots: &[CodexRoot], path: &Path) {
        let error = authorize_rollout(roots, &path.to_string_lossy()).unwrap_err();
        assert!(error.contains("configured Codex home"), "{error}");
    }

    #[test]
    fn rollouts_under_sessions_and_archived_sessions_are_authorized() {
        let (dir, roots) = home_with(&[
            "sessions/2026/01/01/rollout-live.jsonl",
            "archived_sessions/rollout-old.jsonl",
        ]);
        for file in [
            "sessions/2026/01/01/rollout-live.jsonl",
            "archived_sessions/rollout-old.jsonl",
        ] {
            let real = authorize_rollout(&roots, &dir.path().join(file).to_string_lossy()).unwrap();
            assert_eq!(real, dir.path().join(file).canonicalize().unwrap());
        }
    }

    #[test]
    fn jsonl_files_outside_any_home_are_rejected() {
        let (_home, roots) = home_with(&["sessions/rollout-live.jsonl"]);
        let (other, _) = home_with(&["sessions/rollout-elsewhere.jsonl", "private.jsonl"]);
        rejected(
            &roots,
            &other.path().join("sessions/rollout-elsewhere.jsonl"),
        );
        rejected(&roots, &other.path().join("private.jsonl"));
    }

    #[test]
    fn only_rollout_jsonl_files_in_the_session_folders_are_authorized() {
        let (dir, roots) = home_with(&[
            "session_index.jsonl",
            "history.jsonl",
            "sessions/notes.jsonl",
            "sessions/rollout-live.txt",
            "sessions/rollout-live.jsonl.bak",
            "sessions-other/rollout-live.jsonl",
            "archived_sessions/history.jsonl",
        ]);
        std::fs::create_dir_all(dir.path().join("sessions/rollout-dir.jsonl")).unwrap();
        for file in [
            "session_index.jsonl",
            "history.jsonl",
            "sessions/notes.jsonl",
            "sessions/rollout-live.txt",
            "sessions/rollout-live.jsonl.bak",
            "sessions-other/rollout-live.jsonl",
            "archived_sessions/history.jsonl",
            "sessions/rollout-dir.jsonl",
            "sessions",
        ] {
            rejected(&roots, &dir.path().join(file));
        }
    }

    #[test]
    fn dot_dot_segments_cannot_leave_a_home() {
        let (home, roots) = home_with(&["sessions/rollout-live.jsonl"]);
        let (other, _) = home_with(&["rollout-elsewhere.jsonl"]);
        let escape = home
            .path()
            .join("sessions")
            .join("..")
            .join("..")
            .join(other.path().file_name().unwrap())
            .join("rollout-elsewhere.jsonl");
        // Only meaningful when the sibling temp dir really is next to the home.
        if home.path().parent() == other.path().parent() {
            assert!(escape.exists());
            rejected(&roots, &escape);
        }
        // A `..` that stays inside the home is fine: it resolves to a rollout.
        let inside = home.path().join("sessions/../sessions/rollout-live.jsonl");
        assert!(authorize_rollout(&roots, &inside.to_string_lossy()).is_ok());
    }

    #[test]
    fn missing_and_rejected_paths_get_the_same_error() {
        let (dir, roots) = home_with(&["history.jsonl"]);
        let missing = authorize_rollout(
            &roots,
            &dir.path()
                .join("sessions/rollout-x.jsonl")
                .to_string_lossy(),
        );
        let rejected =
            authorize_rollout(&roots, &dir.path().join("history.jsonl").to_string_lossy());
        assert_eq!(missing, rejected);
    }

    #[test]
    fn a_missing_home_authorizes_nothing() {
        let (dir, mut roots) = home_with(&["sessions/rollout-live.jsonl"]);
        roots[0].exists = false;
        rejected(&roots, &dir.path().join("sessions/rollout-live.jsonl"));
        assert!(authorize_rollout(
            &[],
            &dir.path()
                .join("sessions/rollout-live.jsonl")
                .to_string_lossy()
        )
        .is_err());
    }

    #[test]
    fn every_configured_home_is_considered() {
        let (first, mut roots) = home_with(&["sessions/rollout-first.jsonl"]);
        let (second, second_roots) = home_with(&["archived_sessions/rollout-second.jsonl"]);
        roots.extend(second_roots);
        for path in [
            first.path().join("sessions/rollout-first.jsonl"),
            second.path().join("archived_sessions/rollout-second.jsonl"),
        ] {
            assert!(authorize_rollout(&roots, &path.to_string_lossy()).is_ok());
        }
    }

    #[cfg(unix)]
    #[test]
    fn symlinks_are_followed_before_the_home_check() {
        use std::os::unix::fs::symlink;
        let (home, roots) = home_with(&["sessions/rollout-live.jsonl"]);
        let (other, _) = home_with(&[
            "secret.jsonl",
            "rollout-elsewhere.jsonl",
            "dir/rollout-nested.jsonl",
        ]);
        let sessions = home.path().join("sessions");
        // A rollout-looking name inside the home that points at a file outside.
        symlink(
            other.path().join("secret.jsonl"),
            sessions.join("rollout-link.jsonl"),
        )
        .unwrap();
        // A directory symlink that leads out of the home.
        symlink(other.path().join("dir"), sessions.join("linked-dir")).unwrap();
        rejected(&roots, &sessions.join("rollout-link.jsonl"));
        rejected(&roots, &sessions.join("linked-dir/rollout-nested.jsonl"));
        // A link placed outside the home that points at a real rollout inside
        // it resolves to the real file, which is allowed.
        let alias = other.path().join("rollout-alias.jsonl");
        symlink(sessions.join("rollout-live.jsonl"), &alias).unwrap();
        assert_eq!(
            authorize_rollout(&roots, &alias.to_string_lossy()).unwrap(),
            sessions.join("rollout-live.jsonl").canonicalize().unwrap()
        );
        // A rollout in the home that is reached through a symlinked folder
        // that is itself the configured home still works.
        let linked_home = other.path().join("linked-home");
        symlink(home.path(), &linked_home).unwrap();
        let linked_roots = root_candidates(linked_home.to_str(), None, None);
        assert!(authorize_rollout(
            &linked_roots,
            &sessions.join("rollout-live.jsonl").to_string_lossy()
        )
        .is_ok());
        assert!(authorize_rollout(
            &linked_roots,
            &linked_home
                .join("sessions/rollout-live.jsonl")
                .to_string_lossy()
        )
        .is_ok());
    }

    #[test]
    fn wsl_paths_support_default_and_custom_homes() {
        assert_eq!(
            unc_home("ExampleLinux", "/home/demo/.codex").unwrap(),
            r"\\wsl.localhost\ExampleLinux\home\demo\.codex"
        );
        assert_eq!(
            unc_home("ExampleLinux", "/data/codex/").unwrap(),
            r"\\wsl.localhost\ExampleLinux\data\codex"
        );
        assert!(unc_home("ExampleLinux", "relative").is_none());
    }
}
