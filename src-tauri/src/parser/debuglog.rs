use chrono::{DateTime, NaiveDateTime, Utc};
use lazy_static::lazy_static;
use regex::Regex;
use serde::Serialize;
use std::fs;
use std::io::{BufRead, BufReader, Seek, SeekFrom};
use std::path::Path;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub enum DebugLevel {
    Debug,
    Warn,
    Error,
}

impl std::fmt::Display for DebugLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DebugLevel::Debug => write!(f, "DEBUG"),
            DebugLevel::Warn => write!(f, "WARN"),
            DebugLevel::Error => write!(f, "ERROR"),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct DebugEntry {
    pub timestamp: DateTime<Utc>,
    pub level: DebugLevel,
    pub category: String,
    pub message: String,
    pub extra: String,
    pub line_num: usize,
    pub count: usize,
}

lazy_static! {
    static ref DEBUG_LINE_RE: Regex = Regex::new(
        r"^(\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}\.\d{3}Z)\s+\[(DEBUG|WARN|ERROR)\]\s+(.*)$"
    ).unwrap();
    static ref DEBUG_CATEGORY_RE: Regex = Regex::new(r"^\[([^\]]+)\]\s*(.*)$").unwrap();
}

fn parse_level(s: &str) -> DebugLevel {
    match s {
        "WARN" => DebugLevel::Warn,
        "ERROR" => DebugLevel::Error,
        _ => DebugLevel::Debug,
    }
}

/// Read a debug log file from the beginning.
pub fn read_debug_log(path: &str) -> Result<(Vec<DebugEntry>, u64), String> {
    read_debug_log_incremental(path, 0)
}

fn read_debug_log_incremental(path: &str, offset: u64) -> Result<(Vec<DebugEntry>, u64), String> {
    let f = fs::File::open(path).map_err(|e| format!("opening {}: {}", path, e))?;
    let mut reader = BufReader::new(f);
    reader.seek(SeekFrom::Start(offset)).map_err(|e| format!("seeking: {}", e))?;

    let mut entries = Vec::new();
    let mut bytes_read = offset;
    let mut line_num = if offset == 0 { 0 } else { count_lines_before_offset(path, offset) };

    let mut line = String::new();
    loop {
        line.clear();
        let n = reader.read_line(&mut line).map_err(|e| format!("reading: {}", e))?;
        if n == 0 {
            break;
        }
        bytes_read += n as u64;
        line_num += 1;

        let trimmed = line.trim_end();
        if let Some(caps) = DEBUG_LINE_RE.captures(trimmed) {
            let ts_str = caps.get(1).unwrap().as_str();
            let ts = NaiveDateTime::parse_from_str(ts_str, "%Y-%m-%dT%H:%M:%S%.3fZ")
                .map(|naive| DateTime::<Utc>::from_naive_utc_and_offset(naive, Utc))
                .unwrap_or_else(|_| Utc::now());
            let level = parse_level(caps.get(2).unwrap().as_str());
            let body = caps.get(3).unwrap().as_str();

            let (category, message) = if let Some(cm) = DEBUG_CATEGORY_RE.captures(body) {
                (
                    cm.get(1).unwrap().as_str().to_string(),
                    cm.get(2).unwrap().as_str().to_string(),
                )
            } else {
                (String::new(), body.to_string())
            };

            entries.push(DebugEntry {
                timestamp: ts,
                level,
                category,
                message,
                extra: String::new(),
                line_num,
                count: 1,
            });
        } else if !entries.is_empty() && !trimmed.is_empty() {
            let last = entries.last_mut().unwrap();
            if last.extra.is_empty() {
                last.extra = trimmed.to_string();
            } else {
                last.extra.push('\n');
                last.extra.push_str(trimmed);
            }
        }
    }

    Ok((entries, bytes_read))
}

fn count_lines_before_offset(path: &str, offset: u64) -> usize {
    if offset == 0 {
        return 0;
    }
    let f = match fs::File::open(path) {
        Ok(f) => f,
        Err(_) => return 0,
    };
    let mut reader = BufReader::new(f);
    let mut count = 0;
    let mut read = 0u64;
    let mut buf = [0u8; 32 * 1024];
    use std::io::Read;
    while read < offset {
        let to_read = std::cmp::min(buf.len() as u64, offset - read) as usize;
        let n = match reader.read(&mut buf[..to_read]) {
            Ok(0) => break,
            Ok(n) => n,
            Err(_) => break,
        };
        for &byte in &buf[..n] {
            if byte == b'\n' {
                count += 1;
            }
        }
        read += n as u64;
    }
    count
}

/// Returns the debug log file path for a given session JSONL path.
pub fn debug_log_path(session_path: &str) -> String {
    let base = Path::new(session_path)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("");
    if base.is_empty() {
        return String::new();
    }
    let home = match dirs::home_dir() {
        Some(h) => h,
        None => return String::new(),
    };
    let debug_path = home.join(".claude").join("debug").join(format!("{}.txt", base));
    if debug_path.exists() {
        debug_path.to_string_lossy().to_string()
    } else {
        String::new()
    }
}

/// Filter entries by minimum level.
pub fn filter_by_level(entries: &[DebugEntry], min_level: &DebugLevel) -> Vec<DebugEntry> {
    if *min_level == DebugLevel::Debug {
        return entries.to_vec();
    }
    entries.iter().filter(|e| e.level >= *min_level).cloned().collect()
}

/// Filter entries by text query (case-insensitive).
pub fn filter_by_text(entries: &[DebugEntry], query: &str) -> Vec<DebugEntry> {
    if query.is_empty() {
        return entries.to_vec();
    }
    let q = query.to_lowercase();
    entries
        .iter()
        .filter(|e| {
            e.message.to_lowercase().contains(&q)
                || e.category.to_lowercase().contains(&q)
                || e.extra.to_lowercase().contains(&q)
        })
        .cloned()
        .collect()
}

/// Collapse consecutive duplicate entries.
pub fn collapse_duplicates(entries: Vec<DebugEntry>) -> Vec<DebugEntry> {
    if entries.is_empty() {
        return entries;
    }
    let mut result = Vec::new();
    let mut current = entries[0].clone();

    for entry in entries.into_iter().skip(1) {
        if entry.message == current.message && entry.extra.is_empty() && current.extra.is_empty() {
            current.count += 1;
        } else {
            result.push(current);
            current = entry;
        }
    }
    result.push(current);
    result
}
