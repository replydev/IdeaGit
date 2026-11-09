//! The Log's Date column: IntelliJ's "pretty" relative date.
//!
//! Port of `DateFormatUtil.formatPrettyDateTime` (`platform/platform-api/src/com/intellij/util/text/DateFormatUtil.java`):
//! up to 61 minutes ago as "N minutes ago" (`UtilBundle` `date.format.minutes.ago`), then
//! "Today h:mm AM" / "Yesterday h:mm AM", otherwise the date and time.
//!
//! Deviation: the absolute format is the JDK's `en_US` `FormatStyle.SHORT` ("M/d/yy, h:mm AM").
//! Upstream asks `DateTimeFormatManager`, which follows the OS locale settings by default.

use chrono::{DateTime, Datelike, TimeZone};

use crate::strings;

const MINUTE: i64 = 60_000;
const HOUR: i64 = 60 * MINUTE;

/// The texts the format needs, from `UtilBundle`.
pub struct DateTexts {
    pub minutes_ago: String,
    pub today: String,
    pub yesterday: String,
}

impl DateTexts {
    pub fn load() -> DateTexts {
        DateTexts {
            minutes_ago: strings::text("UtilBundle", "date.format.minutes.ago"),
            today: strings::text("UtilBundle", "date.format.today"),
            yesterday: strings::text("UtilBundle", "date.format.yesterday"),
        }
    }
}

/// `formatTime` (`FormatStyle.SHORT`, en_US).
fn format_time<Tz: TimeZone>(t: &DateTime<Tz>) -> String
where
    Tz::Offset: std::fmt::Display,
{
    t.format("%-I:%M\u{202f}%p").to_string()
}

/// `formatPrettyDateTime(time)`: `time` and `now` are epoch milliseconds, shown in `tz`.
pub fn format_pretty<Tz: TimeZone>(time_ms: i64, now_ms: i64, tz: &Tz, texts: &DateTexts) -> String
where
    Tz::Offset: std::fmt::Display,
{
    if time_ms < 0 {
        return String::new();
    }
    let delta = now_ms - time_ms;
    if (0..=HOUR + MINUTE).contains(&delta) {
        let minutes = (delta as f64 / MINUTE as f64).round_ties_even() as i64;
        return strings::format_message(&texts.minutes_ago, &[&minutes.to_string()]);
    }
    let (Some(now), Some(time)) = (
        tz.timestamp_millis_opt(now_ms).single(),
        tz.timestamp_millis_opt(time_ms).single(),
    ) else {
        return String::new();
    };
    if now.date_naive() == time.date_naive() {
        return format!("{} {}", texts.today, format_time(&time));
    }
    let is_yesterday = now
        .date_naive()
        .pred_opt()
        .is_some_and(|d| d == time.date_naive());
    if is_yesterday {
        return format!("{} {}", texts.yesterday, format_time(&time));
    }
    format!(
        "{}/{}/{:02}, {}",
        time.month(),
        time.day(),
        time.year().rem_euclid(100),
        format_time(&time)
    )
}

#[cfg(test)]
mod tests {
    use chrono::FixedOffset;

    use super::*;

    fn texts() -> DateTexts {
        DateTexts {
            minutes_ago: "{0,choice, 0#Moments|1#A minute|2#{0,number} minutes|60#1 hour} ago"
                .into(),
            today: "Today".into(),
            yesterday: "Yesterday".into(),
        }
    }

    fn ms(tz: &FixedOffset, y: i32, mo: u32, d: u32, h: u32, mi: u32) -> i64 {
        tz.with_ymd_and_hms(y, mo, d, h, mi, 0)
            .unwrap()
            .timestamp_millis()
    }

    #[test]
    fn recent_commits_show_minutes() {
        let tz = FixedOffset::east_opt(3600).unwrap();
        let now = ms(&tz, 2026, 11, 8, 15, 0);
        assert_eq!(
            format_pretty(now - 10_000, now, &tz, &texts()),
            "Moments ago"
        );
        assert_eq!(
            format_pretty(now - 42 * MINUTE, now, &tz, &texts()),
            "42 minutes ago"
        );
        assert_eq!(
            format_pretty(now - 61 * MINUTE, now, &tz, &texts()),
            "1 hour ago"
        );
    }

    #[test]
    fn today_and_yesterday_use_local_days() {
        let tz = FixedOffset::east_opt(3600).unwrap();
        let now = ms(&tz, 2026, 11, 8, 15, 0);
        let nbsp = '\u{202f}';
        assert_eq!(
            format_pretty(ms(&tz, 2026, 11, 8, 9, 5), now, &tz, &texts()),
            format!("Today 9:05{nbsp}AM")
        );
        assert_eq!(
            format_pretty(ms(&tz, 2026, 11, 7, 23, 59), now, &tz, &texts()),
            format!("Yesterday 11:59{nbsp}PM")
        );
        assert_eq!(
            format_pretty(
                ms(&tz, 2026, 1, 1, 0, 0),
                ms(&tz, 2026, 1, 1, 10, 0),
                &tz,
                &texts()
            ),
            format!("Today 12:00{nbsp}AM")
        );
        // yesterday across a year boundary
        assert_eq!(
            format_pretty(
                ms(&tz, 2025, 12, 31, 13, 0),
                ms(&tz, 2026, 1, 1, 10, 0),
                &tz,
                &texts()
            ),
            format!("Yesterday 1:00{nbsp}PM")
        );
    }

    #[test]
    fn older_commits_show_the_short_date() {
        let tz = FixedOffset::east_opt(0).unwrap();
        let now = ms(&tz, 2026, 11, 8, 15, 0);
        assert_eq!(
            format_pretty(ms(&tz, 2017, 7, 14, 2, 40), now, &tz, &texts()),
            "7/14/17, 2:40\u{202f}AM"
        );
        assert_eq!(format_pretty(-1, now, &tz, &texts()), "");
    }
}
