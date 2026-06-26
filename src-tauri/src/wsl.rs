//! WSL (Windows Subsystem for Linux) integration.
//!
//! Lets the desktop app discover Claude Code sessions that live *inside* WSL
//! distributions. Claude Code running in a distro stores its projects under the
//! Linux home directory (`~/.claude/projects`); from Windows those are reachable
//! through the `\\wsl.localhost\<distro>\...` UNC share. This module enumerates
//! installed distros, resolves each one's projects directory, and aggregates the
//! per-project subdirectories alongside the host's own projects directory.
//!
//! All `wsl.exe` invocation is gated to Windows. On other platforms the spawn
//! helpers are no-ops, so configured distros simply contribute nothing — the
//! pure parsing/path helpers remain testable everywhere.

use std::collections::HashMap;
use std::path::Path;
use std::sync::{Mutex, OnceLock};

/// Decode raw bytes emitted by `wsl.exe`. `wsl.exe --list` writes UTF-16LE on
/// Windows, whereas stdout piped from a command run *inside* a distro is UTF-8.
/// Detect UTF-16 by the presence of NUL bytes and decode accordingly.
fn decode_wsl_bytes(bytes: &[u8]) -> String {
    if bytes.contains(&0) {
        let u16s: Vec<u16> = bytes
            .chunks_exact(2)
            .map(|c| u16::from_le_bytes([c[0], c[1]]))
            .collect();
        String::from_utf16_lossy(&u16s)
    } else {
        String::from_utf8_lossy(bytes).into_owned()
    }
}

/// Parse the output of `wsl --list --quiet` into distro names. Strips BOMs, NULs,
/// and carriage returns, drops blank lines, and ignores lines containing interior
/// whitespace (e.g. the "no installed distributions" message WSL prints when none
/// exist — distro names are always single tokens).
fn parse_distro_list(raw: &str) -> Vec<String> {
    raw.lines()
        .map(|l| l.trim_matches(|c: char| c.is_whitespace() || c == '\u{feff}' || c == '\0'))
        .filter(|l| !l.is_empty() && !l.contains(char::is_whitespace))
        .map(str::to_string)
        .collect()
}

/// Build the Windows UNC path to a distro's Claude projects directory from a
/// POSIX home directory.
/// e.g. `("Ubuntu", "/home/nat")` -> `\\wsl.localhost\Ubuntu\home\nat\.claude\projects`.
fn unc_projects_path(distro: &str, posix_home: &str) -> String {
    let home = posix_home
        .trim()
        .trim_end_matches('/')
        .trim_start_matches('/')
        .replace('/', "\\");
    if home.is_empty() {
        format!(r"\\wsl.localhost\{distro}\.claude\projects")
    } else {
        format!(r"\\wsl.localhost\{distro}\{home}\.claude\projects")
    }
}

/// Run `wsl.exe` with the given args and return stdout on success.
#[cfg(target_os = "windows")]
fn run_wsl(args: &[&str]) -> Option<Vec<u8>> {
    use std::os::windows::process::CommandExt;
    // CREATE_NO_WINDOW — keep wsl.exe from flashing a console window.
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let output = std::process::Command::new("wsl.exe")
        .args(args)
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .ok()?;
    if output.status.success() {
        Some(output.stdout)
    } else {
        None
    }
}

#[cfg(not(target_os = "windows"))]
fn run_wsl(_args: &[&str]) -> Option<Vec<u8>> {
    None
}

/// List installed WSL distributions. Returns an empty list on non-Windows
/// platforms or when WSL is not installed / has no distros.
pub fn list_distros() -> Vec<String> {
    match run_wsl(&["--list", "--quiet"]) {
        Some(bytes) => parse_distro_list(&decode_wsl_bytes(&bytes)),
        None => Vec::new(),
    }
}

/// Process-wide cache of successfully resolved distro projects directories.
/// A distro's home directory is effectively immutable for the app's lifetime, so
/// caching avoids re-spawning `wsl.exe` on every filesystem-triggered rescan.
/// Only successful resolutions are cached — an unreachable distro keeps retrying.
fn resolve_cache() -> &'static Mutex<HashMap<String, String>> {
    static CACHE: OnceLock<Mutex<HashMap<String, String>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Resolve the Windows-accessible Claude projects directory for a WSL distro,
/// or `None` if the distro can't be reached or reports no home directory.
pub fn resolve_distro_projects_dir(distro: &str) -> Option<String> {
    if let Ok(cache) = resolve_cache().lock() {
        if let Some(dir) = cache.get(distro) {
            return Some(dir.clone());
        }
    }
    let resolved = resolve_distro_projects_dir_uncached(distro)?;
    if let Ok(mut cache) = resolve_cache().lock() {
        cache.insert(distro.to_string(), resolved.clone());
    }
    Some(resolved)
}

fn resolve_distro_projects_dir_uncached(distro: &str) -> Option<String> {
    let bytes = run_wsl(&["-d", distro, "--", "sh", "-c", "echo $HOME"])?;
    let home = decode_wsl_bytes(&bytes);
    let home = home.trim();
    if !home.starts_with('/') {
        return None;
    }
    Some(unc_projects_path(distro, home))
}

/// List the immediate subdirectories of `dir` as path strings.
fn list_subdirs(dir: &Path) -> Vec<String> {
    let mut dirs = Vec::new();
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            if entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                dirs.push(entry.path().to_string_lossy().to_string());
            }
        }
    }
    dirs
}

/// Collect every project directory to scan for sessions: the host's configured
/// (or default) projects directory plus each configured WSL distro's projects
/// directory. Each returned entry is an encoded-path project subdirectory.
pub fn collect_project_dirs(configured: Option<&str>, wsl_distros: &[String]) -> Vec<String> {
    let mut dirs = Vec::new();

    if let Ok(base) = crate::parser::session::claude_projects_dir(configured) {
        if base.exists() {
            dirs.extend(list_subdirs(&base));
        }
    }

    for distro in wsl_distros {
        if let Some(projects) = resolve_distro_projects_dir(distro) {
            let p = Path::new(&projects);
            if p.exists() {
                dirs.extend(list_subdirs(p));
            }
        }
    }

    dirs
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- decode_wsl_bytes ---

    #[test]
    fn decode_utf8_passthrough() {
        assert_eq!(decode_wsl_bytes(b"/home/nat\n"), "/home/nat\n");
    }

    #[test]
    fn decode_utf16le_with_nuls() {
        // "Ub" encoded UTF-16LE: 0x55 0x00 0x62 0x00
        let bytes = [0x55u8, 0x00, 0x62, 0x00];
        assert_eq!(decode_wsl_bytes(&bytes), "Ub");
    }

    // --- parse_distro_list ---

    #[test]
    fn parse_distro_list_basic() {
        let raw = "Ubuntu\r\nDebian\r\nkali-linux\r\n";
        assert_eq!(
            parse_distro_list(raw),
            vec!["Ubuntu", "Debian", "kali-linux"]
        );
    }

    #[test]
    fn parse_distro_list_strips_bom_and_blanks() {
        let raw = "\u{feff}Ubuntu\n\n  Debian  \n";
        assert_eq!(parse_distro_list(raw), vec!["Ubuntu", "Debian"]);
    }

    #[test]
    fn parse_distro_list_ignores_sentence_lines() {
        // The message WSL prints when there are no distros has interior spaces.
        let raw = "Windows Subsystem for Linux has no installed distributions.\n";
        assert!(parse_distro_list(raw).is_empty());
    }

    // --- unc_projects_path ---

    #[test]
    fn unc_path_for_standard_home() {
        assert_eq!(
            unc_projects_path("Ubuntu", "/home/nat"),
            r"\\wsl.localhost\Ubuntu\home\nat\.claude\projects"
        );
    }

    #[test]
    fn unc_path_trims_trailing_slash_and_newline() {
        assert_eq!(
            unc_projects_path("Debian", "/root/\n"),
            r"\\wsl.localhost\Debian\root\.claude\projects"
        );
    }

    #[test]
    fn unc_path_handles_root_home() {
        assert_eq!(
            unc_projects_path("Alpine", "/"),
            r"\\wsl.localhost\Alpine\.claude\projects"
        );
    }

    // --- resolve_distro_projects_dir (non-Windows: run_wsl is a no-op) ---

    #[cfg(not(target_os = "windows"))]
    #[test]
    fn resolve_returns_none_off_windows() {
        assert_eq!(resolve_distro_projects_dir("Ubuntu"), None);
    }

    // --- list_distros (non-Windows) ---

    #[cfg(not(target_os = "windows"))]
    #[test]
    fn list_distros_empty_off_windows() {
        assert!(list_distros().is_empty());
    }

    // --- collect_project_dirs ---

    #[test]
    fn collect_lists_base_subdirs_no_distros() {
        let tmp = std::env::temp_dir().join("cctrace-wsl-collect-test");
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(tmp.join("-home-me-proj-a")).unwrap();
        std::fs::create_dir_all(tmp.join("-home-me-proj-b")).unwrap();
        // A stray file should be ignored.
        std::fs::write(tmp.join("not-a-dir.txt"), "x").unwrap();

        let dirs = collect_project_dirs(Some(tmp.to_str().unwrap()), &[]);
        assert_eq!(dirs.len(), 2);
        assert!(dirs.iter().any(|d| d.ends_with("-home-me-proj-a")));
        assert!(dirs.iter().any(|d| d.ends_with("-home-me-proj-b")));

        std::fs::remove_dir_all(&tmp).ok();
    }

    #[test]
    fn collect_skips_missing_base_dir() {
        // A non-existent configured path falls back to the default
        // ~/.claude/projects, which may or may not exist on the test machine.
        // Either way it must not panic and must never yield empty path strings.
        let dirs = collect_project_dirs(Some("/no/such/path/at/all/xyz"), &[]);
        assert!(dirs.iter().all(|d| !d.is_empty()));
    }
}
