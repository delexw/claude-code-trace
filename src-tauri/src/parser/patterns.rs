use lazy_static::lazy_static;
use regex::Regex;
use serde::Deserialize;

// Tag constants matching the TypeScript messageTags.ts.
pub const LOCAL_COMMAND_STDOUT_TAG: &str = "<local-command-stdout>";
pub const LOCAL_COMMAND_STDERR_TAG: &str = "<local-command-stderr>";

// Bash mode tags -- inline command execution via !bash in Claude Code.
pub const BASH_STDOUT_TAG: &str = "<bash-stdout>";
pub const BASH_STDERR_TAG: &str = "<bash-stderr>";
pub const TASK_NOTIFICATION_TAG: &str = "<task-notification>";

lazy_static! {
    // Command extraction regexes -- used by sanitize and session.
    pub static ref RE_COMMAND_NAME: Regex =
        Regex::new(r"<command-name>/([^<]+)</command-name>").unwrap();
    pub static ref RE_COMMAND_ARGS: Regex =
        Regex::new(r"<command-args>([^<]*)</command-args>").unwrap();
    pub static ref RE_STDOUT: Regex =
        Regex::new(r"(?is)<local-command-stdout>(.*?)</local-command-stdout>").unwrap();
    pub static ref RE_STDERR: Regex =
        Regex::new(r"(?is)<local-command-stderr>(.*?)</local-command-stderr>").unwrap();

    // Bash mode regexes -- used by classify and sanitize.
    pub static ref RE_BASH_STDOUT: Regex =
        Regex::new(r"(?is)<bash-stdout>(.*?)</bash-stdout>").unwrap();
    pub static ref RE_BASH_STDERR: Regex =
        Regex::new(r"(?is)<bash-stderr>(.*?)</bash-stderr>").unwrap();
    pub static ref RE_BASH_INPUT: Regex =
        Regex::new(r"(?is)<bash-input>(.*?)</bash-input>").unwrap();
    pub static ref RE_TASK_NOTIFY_SUMMARY: Regex =
        Regex::new(r"(?is)<summary>(.*?)</summary>").unwrap();
    pub static ref RE_TASK_NOTIFY_STATUS: Regex =
        Regex::new(r"(?is)<status>(.*?)</status>").unwrap();

    // Teammate message regexes -- used by classify, session, and subagent.
    pub static ref TEAMMATE_MESSAGE_RE: Regex =
        Regex::new(r#"^<teammate-message\s+teammate_id="[^"]+""#).unwrap();
    pub static ref TEAMMATE_ID_RE: Regex =
        Regex::new(r#"teammate_id="([^"]+)""#).unwrap();
    pub static ref TEAMMATE_CONTENT_RE: Regex =
        Regex::new(r"(?s)<teammate-message[^>]*>(.*?)</teammate-message>").unwrap();
    pub static ref TEAMMATE_SUMMARY_RE: Regex =
        Regex::new(r#"<teammate-message[^>]*\bsummary="([^"]+)""#).unwrap();
    pub static ref TEAMMATE_COLOR_RE: Regex =
        Regex::new(r#"<teammate-message[^>]*\bcolor="([^"]+)""#).unwrap();
    pub static ref TEAMMATE_PROTOCOL_RE: Regex =
        Regex::new(r#"^\s*\{\s*"type"\s*:\s*"(idle_notification|shutdown_approved|shutdown_request|teammate_terminated|task_assignment)""#).unwrap();

    // Noise tag patterns - system-generated metadata stripped from display content.
    pub static ref NOISE_TAG_PATTERNS: Vec<Regex> = vec![
        Regex::new(r"(?is)<local-command-caveat>.*?</local-command-caveat>").unwrap(),
        Regex::new(r"(?is)<system-reminder>.*?</system-reminder>").unwrap(),
    ];

    // Command tag patterns - removed after extracting display form.
    pub static ref COMMAND_TAG_PATTERNS: Vec<Regex> = vec![
        Regex::new(r"(?is)<command-name>.*?</command-name>").unwrap(),
        Regex::new(r"(?is)<command-message>.*?</command-message>").unwrap(),
        Regex::new(r"(?is)<command-args>.*?</command-args>").unwrap(),
    ];
}

/// contentBlockJSON is the common shape for partially unmarshaling JSONL content blocks.
#[derive(Debug, Deserialize, Default)]
pub struct ContentBlockJSON {
    #[serde(default, rename = "type")]
    pub block_type: String,
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub text: String,
    #[serde(default)]
    pub thinking: String,
    #[serde(default)]
    pub input: Option<serde_json::Value>,
    #[serde(default, rename = "tool_use_id")]
    pub tool_use_id: String,
    #[serde(default)]
    pub content: Option<serde_json::Value>,
    #[serde(default, rename = "is_error")]
    pub is_error: bool,
}

/// textBlockJSON is a minimal content block for extracting text content.
#[derive(Debug, Deserialize, Default)]
pub struct TextBlockJSON {
    #[serde(default, rename = "type")]
    pub block_type: String,
    #[serde(default)]
    pub text: String,
}
