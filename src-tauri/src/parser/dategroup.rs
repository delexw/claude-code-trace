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
