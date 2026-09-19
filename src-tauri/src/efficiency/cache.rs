use base64::Engine;
use sha2::{Digest, Sha256};
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

use super::{
    EfficiencySummary, SessionEfficiencyAnalysis, ANALYSIS_VERSION, DECISION_SET_VERSION,
    SCORE_FORMULA_VERSION,
};

pub fn transcript_fingerprint(path: &str) -> Result<String, String> {
    let mut file = fs::File::open(path).map_err(|error| error.to_string())?;
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let count = file.read(&mut buffer).map_err(|error| error.to_string())?;
        if count == 0 {
            break;
        }
        hasher.update(&buffer[..count]);
    }
    Ok(base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(hasher.finalize()))
}

fn cache_root() -> Result<PathBuf, String> {
    crate::settings::config_root()
        .map(|root| root.join("analysis"))
        .ok_or_else(|| "no config directory".to_string())
}

fn cache_path_in(root: &Path, session_path: &str) -> PathBuf {
    let mut hasher = Sha256::new();
    hasher.update(session_path.as_bytes());
    root.join(format!(
        "{}.json",
        base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(hasher.finalize())
    ))
}

fn cache_path(session_path: &str) -> Result<PathBuf, String> {
    Ok(cache_path_in(&cache_root()?, session_path))
}

pub fn write(analysis: &SessionEfficiencyAnalysis) -> Result<(), String> {
    let path = cache_path(&analysis.session_path)?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    let contents = serde_json::to_string_pretty(analysis).map_err(|error| error.to_string())?;
    fs::write(path, contents).map_err(|error| error.to_string())
}

fn read_unchecked(session_path: &str) -> Result<Option<SessionEfficiencyAnalysis>, String> {
    let path = cache_path(session_path)?;
    if !path.exists() {
        return Ok(None);
    }
    let contents = fs::read_to_string(path).map_err(|error| error.to_string())?;
    serde_json::from_str(&contents)
        .map(Some)
        .map_err(|error| error.to_string())
}

pub fn read(session_path: &str) -> Result<Option<SessionEfficiencyAnalysis>, String> {
    let Some(mut analysis) = read_unchecked(session_path)? else {
        return Ok(None);
    };
    let current_fingerprint = transcript_fingerprint(session_path)?;
    analysis.stale = current_fingerprint != analysis.transcript_fingerprint
        || analysis.analysis_version != ANALYSIS_VERSION
        || analysis.decision_set_version != DECISION_SET_VERSION
        || analysis.score_formula_version != SCORE_FORMULA_VERSION;
    Ok(Some(analysis))
}

pub fn delete(session_path: &str) -> Result<(), String> {
    let path = cache_path(session_path)?;
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.to_string()),
    }
}

pub fn list_summaries() -> Result<Vec<EfficiencySummary>, String> {
    let root = cache_root()?;
    let entries = match fs::read_dir(root) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(error.to_string()),
    };
    let mut summaries = Vec::new();
    for entry in entries.flatten() {
        let Ok(contents) = fs::read_to_string(entry.path()) else {
            continue;
        };
        let Ok(mut analysis) = serde_json::from_str::<SessionEfficiencyAnalysis>(&contents) else {
            continue;
        };
        analysis.stale = transcript_fingerprint(&analysis.session_path)
            .map(|fingerprint| fingerprint != analysis.transcript_fingerprint)
            .unwrap_or(true)
            || analysis.analysis_version != ANALYSIS_VERSION
            || analysis.decision_set_version != DECISION_SET_VERSION
            || analysis.score_formula_version != SCORE_FORMULA_VERSION;
        summaries.push(EfficiencySummary {
            session_path: analysis.session_path,
            score: analysis.score,
            analyzed_at: analysis.analyzed_at,
            stale: analysis.stale,
        });
    }
    Ok(summaries)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cache_filename_never_contains_the_session_path() {
        let root = Path::new("/cache");
        let path = cache_path_in(root, "/Users/example/private/session.jsonl");
        let rendered = path.to_string_lossy();
        assert!(rendered.starts_with("/cache/"));
        assert!(!rendered.contains("Users"));
        assert!(rendered.ends_with(".json"));
    }

    #[test]
    fn fingerprint_changes_with_transcript_content() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("session.jsonl");
        fs::write(&path, "one").unwrap();
        let first = transcript_fingerprint(path.to_str().unwrap()).unwrap();
        fs::write(&path, "two").unwrap();
        let second = transcript_fingerprint(path.to_str().unwrap()).unwrap();
        assert_ne!(first, second);
    }
}
