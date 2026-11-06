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
