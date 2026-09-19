use lazy_static::lazy_static;
use regex::{Captures, Regex};

use super::EfficiencyInput;

const REDACTED: &str = "[REDACTED]";

lazy_static! {
    static ref BEARER: Regex = Regex::new(r"(?i)\b(bearer\s+)[A-Za-z0-9._~+/=-]{8,}").unwrap();
    static ref NAMED_SECRET: Regex = Regex::new(
        r#"(?i)\b(api[_-]?key|access[_-]?token|auth(?:orization)?|password|passwd|secret|cookie)\b(\s*[=:]\s*|\"\s*:\s*\")[^\s,;\"']+"#,
    ).unwrap();
    static ref URL_SECRET: Regex = Regex::new(
        r"(?i)([?&](?:access_token|api_key|token|key)=)[^&#\s]+"
    ).unwrap();
    static ref PRIVATE_KEY: Regex = Regex::new(
        r"(?s)-----BEGIN [A-Z ]*PRIVATE KEY-----.*?-----END [A-Z ]*PRIVATE KEY-----"
    ).unwrap();
    static ref ENV_ASSIGNMENT: Regex = Regex::new(
        r"(?m)\b([A-Z][A-Z0-9_]*(?:KEY|TOKEN|SECRET|PASSWORD|COOKIE))=([^\s]+)"
    ).unwrap();
}

pub fn redact_text(value: &str) -> String {
    let value = PRIVATE_KEY.replace_all(value, REDACTED);
    let value = BEARER.replace_all(&value, |captures: &Captures<'_>| {
        format!("{}{}", &captures[1], REDACTED)
    });
    let value = NAMED_SECRET.replace_all(&value, |captures: &Captures<'_>| {
        format!("{}{}{}", &captures[1], &captures[2], REDACTED)
    });
    let value = URL_SECRET.replace_all(&value, |captures: &Captures<'_>| {
        format!("{}{}", &captures[1], REDACTED)
    });
    let value = ENV_ASSIGNMENT.replace_all(&value, |captures: &Captures<'_>| {
        format!("{}={}", &captures[1], REDACTED)
    });
    if let Some(home) = dirs::home_dir().and_then(|path| path.to_str().map(str::to_owned)) {
        value.replace(&home, "~")
    } else {
        value.into_owned()
    }
}

pub fn redact_input(mut input: EfficiencyInput) -> EfficiencyInput {
    input.task.first_user_message = redact_text(&input.task.first_user_message);
    for action in &mut input.actions {
        action.summary = redact_text(&action.summary);
    }
    for excerpt in &mut input.selected_excerpts {
        *excerpt = redact_text(excerpt);
    }
    input
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redacts_common_secret_shapes() {
        let raw = "Authorization: Bearer abcdefghijklmnop API_KEY=sk-secret password=hunter2 https://x.test?a=1&access_token=abc123456";
        let redacted = redact_text(raw);
        assert!(!redacted.contains("abcdefghijklmnop"));
        assert!(!redacted.contains("sk-secret"));
        assert!(!redacted.contains("hunter2"));
        assert!(!redacted.contains("abc123456"));
        assert!(redacted.matches(REDACTED).count() >= 4);
    }

    #[test]
    fn redacts_private_keys() {
        let redacted =
            redact_text("-----BEGIN PRIVATE KEY-----\nvery-secret\n-----END PRIVATE KEY-----");
        assert_eq!(redacted, REDACTED);
    }
}
