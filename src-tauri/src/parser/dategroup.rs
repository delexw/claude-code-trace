use chrono::{DateTime, Local, TimeZone};
use serde::Serialize;

use super::session::SessionInfo;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize)]
pub enum DateCategory {
    Today,
    Yesterday,
    ThisWeek,
    ThisMonth,
    Older,
}

#[derive(Debug, Clone, Serialize)]
pub struct DateGroup {
    pub category: DateCategory,
    pub sessions: Vec<SessionInfo>,
}

/// Group sessions into date categories based on ModTime.
pub fn group_sessions_by_date(sessions: &[SessionInfo]) -> Vec<DateGroup> {
    let now = Local::now();
    let today_start = now
        .date_naive()
        .and_hms_opt(0, 0, 0)
        .map(|dt| Local.from_local_datetime(&dt).unwrap())
        .unwrap_or(now);
    let yesterday_start = today_start - chrono::Duration::days(1);
    let week_start = today_start - chrono::Duration::days(7);
    let month_start = today_start - chrono::Duration::days(30);

    let categories = [
        DateCategory::Today,
        DateCategory::Yesterday,
        DateCategory::ThisWeek,
        DateCategory::ThisMonth,
        DateCategory::Older,
    ];

    let mut buckets: std::collections::HashMap<DateCategory, Vec<SessionInfo>> =
        std::collections::HashMap::new();

    for s in sessions {
        let t: DateTime<Local> = s.mod_time.with_timezone(&Local);
        let cat = if t >= today_start {
            DateCategory::Today
        } else if t >= yesterday_start {
            DateCategory::Yesterday
        } else if t >= week_start {
            DateCategory::ThisWeek
        } else if t >= month_start {
            DateCategory::ThisMonth
        } else {
            DateCategory::Older
        };
        buckets.entry(cat).or_default().push(s.clone());
    }

    let mut groups = Vec::new();
    for cat in categories {
        if let Some(sessions) = buckets.remove(&cat) {
            if !sessions.is_empty() {
                groups.push(DateGroup {
                    category: cat,
                    sessions,
                });
            }
        }
    }
    groups
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{Duration, Local, TimeZone, Utc};

    fn make_session(mod_time: DateTime<Utc>) -> SessionInfo {
        SessionInfo {
            session_id: format!("session-{}", mod_time.timestamp()),
            path: "/tmp/test.jsonl".to_string(),
            first_message: "test".to_string(),
            recap: None,
            name: None,
            mod_time,
            turn_count: 1,
            model: "claude-sonnet-4-20250514".to_string(),
            total_tokens: 100,
            input_tokens: 50,
            output_tokens: 50,
            cache_read_tokens: 0,
            cache_creation_tokens: 0,
            cost_usd: 0.01,
            duration_ms: 1000,
            cwd: "/tmp".to_string(),
            git_branch: "main".to_string(),
            permission_mode: "default".to_string(),
            is_ongoing: false,
        }
    }

    #[test]
    fn empty_sessions_returns_empty_groups() {
        let groups = group_sessions_by_date(&[]);
        assert!(groups.is_empty());
    }

    #[test]
    fn session_from_today_goes_in_today() {
        let now = Utc::now();
        let sessions = vec![make_session(now)];
        let groups = group_sessions_by_date(&sessions);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].category, DateCategory::Today);
        assert_eq!(groups[0].sessions.len(), 1);
    }

    #[test]
    fn session_from_yesterday_goes_in_yesterday() {
        // Use a time well into yesterday (noon yesterday in local time)
        let yesterday = Utc::now() - Duration::hours(30);
        let sessions = vec![make_session(yesterday)];
        let groups = group_sessions_by_date(&sessions);
        assert!(!groups.is_empty());
        // Should be Yesterday or ThisWeek depending on time, but definitely not Today
        assert_ne!(groups[0].category, DateCategory::Today);
    }

    #[test]
    fn old_session_goes_in_older() {
        let old = Utc::now() - Duration::days(60);
        let sessions = vec![make_session(old)];
        let groups = group_sessions_by_date(&sessions);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].category, DateCategory::Older);
    }

    #[test]
    fn groups_are_in_chronological_order() {
        let now = Utc::now();
        let old = now - Duration::days(60);
        let sessions = vec![make_session(old), make_session(now)];
        let groups = group_sessions_by_date(&sessions);
        assert!(groups.len() >= 2);
        // Today should come before Older
        let today_pos = groups
            .iter()
            .position(|g| g.category == DateCategory::Today);
        let older_pos = groups
            .iter()
            .position(|g| g.category == DateCategory::Older);
        assert!(today_pos.unwrap() < older_pos.unwrap());
    }

    #[test]
    fn multiple_sessions_same_category_grouped() {
        // Anchor to noon today to avoid midnight boundary flakiness
        let noon_utc = Local::now()
            .date_naive()
            .and_hms_opt(12, 0, 0)
            .map(|dt| Local.from_local_datetime(&dt).unwrap().with_timezone(&Utc))
            .unwrap_or_else(Utc::now);
        let also_noon = noon_utc - Duration::minutes(5);
        let sessions = vec![make_session(noon_utc), make_session(also_noon)];
        let groups = group_sessions_by_date(&sessions);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].category, DateCategory::Today);
        assert_eq!(groups[0].sessions.len(), 2);
    }
}
