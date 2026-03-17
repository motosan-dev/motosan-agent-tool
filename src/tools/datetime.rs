use std::future::Future;
use std::pin::Pin;

use chrono::{Datelike, Duration, NaiveDate, NaiveTime, TimeZone, Utc, Weekday};
use chrono_tz::Tz;
use serde::Deserialize;
use serde_json::json;

use crate::{Tool, ToolContext, ToolDef, ToolResult};

/// A built-in tool for date/time operations.
///
/// Supports three functions:
/// - `get_current_datetime` — current time in a given timezone
/// - `date_add` — add an offset to a base date
/// - `date_diff` — difference between two dates
pub struct DatetimeTool;

#[derive(Debug, Deserialize)]
struct DatetimeInput {
    function: String,
    #[serde(default)]
    timezone: Option<String>,
    #[serde(default)]
    date: Option<String>,
    #[serde(default)]
    offset: Option<String>,
    #[serde(default)]
    from: Option<String>,
    #[serde(default)]
    to: Option<String>,
}

impl Default for DatetimeTool {
    fn default() -> Self {
        Self::new()
    }
}

impl DatetimeTool {
    pub fn new() -> Self {
        Self
    }
}

impl Tool for DatetimeTool {
    fn def(&self) -> ToolDef {
        ToolDef {
            name: "datetime".to_string(),
            description: "Date and time utilities. Supports getting the current datetime, \
                adding offsets to dates, and calculating differences between dates."
                .to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "function": {
                        "type": "string",
                        "enum": ["get_current_datetime", "date_add", "date_diff"],
                        "description": "The datetime function to call"
                    },
                    "timezone": {
                        "type": "string",
                        "description": "IANA timezone (e.g. \"Asia/Taipei\", \"US/Eastern\"). Defaults to UTC."
                    },
                    "date": {
                        "type": "string",
                        "description": "Base date for date_add in YYYY-MM-DD format"
                    },
                    "offset": {
                        "type": "string",
                        "description": "Offset for date_add: \"+1d\", \"-7d\", \"+2w\", \"+1M\", \"next monday\", etc."
                    },
                    "from": {
                        "type": "string",
                        "description": "Start date for date_diff in YYYY-MM-DD format"
                    },
                    "to": {
                        "type": "string",
                        "description": "End date for date_diff in YYYY-MM-DD format"
                    }
                },
                "required": ["function"]
            }),
        }
    }

    fn call(
        &self,
        args: serde_json::Value,
        _ctx: &ToolContext,
    ) -> Pin<Box<dyn Future<Output = ToolResult> + Send + '_>> {
        Box::pin(async move {
            let input: DatetimeInput = match serde_json::from_value(args) {
                Ok(v) => v,
                Err(e) => return ToolResult::error(format!("Invalid input: {e}")),
            };

            match input.function.as_str() {
                "get_current_datetime" => handle_get_current_datetime(&input),
                "date_add" => handle_date_add(&input),
                "date_diff" => handle_date_diff(&input),
                other => ToolResult::error(format!(
                    "Unknown function: {other}. Expected one of: get_current_datetime, date_add, date_diff"
                )),
            }
        })
    }
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

fn resolve_tz(tz_str: Option<&str>) -> Result<Tz, ToolResult> {
    let tz_name = tz_str.unwrap_or("UTC");
    tz_name.parse::<Tz>().map_err(|_| {
        ToolResult::error(format!("Unknown timezone: {tz_name}"))
    })
}

fn handle_get_current_datetime(input: &DatetimeInput) -> ToolResult {
    let tz = match resolve_tz(input.timezone.as_deref()) {
        Ok(tz) => tz,
        Err(r) => return r,
    };

    let now = Utc::now().with_timezone(&tz);
    let iso = now.to_rfc3339();
    let date = now.format("%Y-%m-%d").to_string();
    let time = now.format("%H:%M").to_string();
    let weekday = format!("{}", now.format("%A"));
    let human = format_human_datetime(&now);

    ToolResult::json(json!({
        "iso": iso,
        "date": date,
        "time": time,
        "weekday": weekday,
        "human": human,
    }))
}

fn handle_date_add(input: &DatetimeInput) -> ToolResult {
    let tz = match resolve_tz(input.timezone.as_deref()) {
        Ok(tz) => tz,
        Err(r) => return r,
    };

    let date_str = match &input.date {
        Some(d) => d.as_str(),
        None => return ToolResult::error("date_add requires a \"date\" field (YYYY-MM-DD)"),
    };
    let offset_str = match &input.offset {
        Some(o) => o.as_str(),
        None => return ToolResult::error("date_add requires an \"offset\" field"),
    };

    let base = match NaiveDate::parse_from_str(date_str, "%Y-%m-%d") {
        Ok(d) => d,
        Err(e) => return ToolResult::error(format!("Invalid date \"{date_str}\": {e}")),
    };

    let result_date = match parse_offset(base, offset_str) {
        Ok(d) => d,
        Err(msg) => return ToolResult::error(msg),
    };

    let dt = tz
        .from_local_datetime(&result_date.and_time(NaiveTime::from_hms_opt(0, 0, 0).unwrap()))
        .single();
    let dt = match dt {
        Some(d) => d,
        None => return ToolResult::error("Ambiguous or invalid local datetime for timezone"),
    };

    let iso = dt.to_rfc3339();
    let date = dt.format("%Y-%m-%d").to_string();
    let weekday = format!("{}", dt.format("%A"));
    let human = format_human_date(&dt);

    ToolResult::json(json!({
        "iso": iso,
        "date": date,
        "weekday": weekday,
        "human": human,
    }))
}

fn handle_date_diff(input: &DatetimeInput) -> ToolResult {
    let from_str = match &input.from {
        Some(f) => f.as_str(),
        None => return ToolResult::error("date_diff requires a \"from\" field (YYYY-MM-DD)"),
    };
    let to_str = match &input.to {
        Some(t) => t.as_str(),
        None => return ToolResult::error("date_diff requires a \"to\" field (YYYY-MM-DD)"),
    };

    let from = match NaiveDate::parse_from_str(from_str, "%Y-%m-%d") {
        Ok(d) => d,
        Err(e) => return ToolResult::error(format!("Invalid from date \"{from_str}\": {e}")),
    };
    let to = match NaiveDate::parse_from_str(to_str, "%Y-%m-%d") {
        Ok(d) => d,
        Err(e) => return ToolResult::error(format!("Invalid to date \"{to_str}\": {e}")),
    };

    let days = (to - from).num_days();
    let abs_days = days.unsigned_abs();
    let weeks = abs_days / 7;
    let months = approximate_months(from, to);
    let human = format_human_diff(days);

    ToolResult::json(json!({
        "days": days,
        "weeks": weeks,
        "months": months,
        "human": human,
    }))
}

// ---------------------------------------------------------------------------
// Offset parsing
// ---------------------------------------------------------------------------

fn parse_offset(base: NaiveDate, offset: &str) -> Result<NaiveDate, String> {
    let trimmed = offset.trim();

    // Handle "next <weekday>"
    if let Some(rest) = trimmed.strip_prefix("next ") {
        let target_weekday = parse_weekday(rest.trim())?;
        let mut d = base + Duration::days(1);
        while d.weekday() != target_weekday {
            d += Duration::days(1);
        }
        return Ok(d);
    }

    // Handle "+Nd", "-Nd", "+Nw", "+NM"
    let (sign, rest) = if let Some(r) = trimmed.strip_prefix('+') {
        (1i64, r)
    } else if let Some(r) = trimmed.strip_prefix('-') {
        (-1i64, r)
    } else {
        return Err(format!(
            "Invalid offset \"{offset}\": expected format like +1d, -7d, +2w, +1M, or \"next monday\""
        ));
    };

    // Split number from unit
    let (num_str, unit) = rest
        .char_indices()
        .find(|(_, c)| c.is_alphabetic())
        .map(|(i, _)| (&rest[..i], &rest[i..]))
        .ok_or_else(|| format!("Invalid offset \"{offset}\": missing unit (d/w/M)"))?;

    let n: i64 = num_str
        .parse()
        .map_err(|_| format!("Invalid offset \"{offset}\": \"{num_str}\" is not a number"))?;

    match unit {
        "d" => Ok(base + Duration::days(sign * n)),
        "w" => Ok(base + Duration::weeks(sign * n)),
        "M" => add_months(base, sign * n),
        _ => Err(format!(
            "Invalid offset unit \"{unit}\": expected d (days), w (weeks), or M (months)"
        )),
    }
}

fn parse_weekday(s: &str) -> Result<Weekday, String> {
    match s.to_lowercase().as_str() {
        "monday" | "mon" => Ok(Weekday::Mon),
        "tuesday" | "tue" => Ok(Weekday::Tue),
        "wednesday" | "wed" => Ok(Weekday::Wed),
        "thursday" | "thu" => Ok(Weekday::Thu),
        "friday" | "fri" => Ok(Weekday::Fri),
        "saturday" | "sat" => Ok(Weekday::Sat),
        "sunday" | "sun" => Ok(Weekday::Sun),
        _ => Err(format!("Unknown weekday: {s}")),
    }
}

fn add_months(base: NaiveDate, months: i64) -> Result<NaiveDate, String> {
    let total_months = base.year() as i64 * 12 + base.month0() as i64 + months;
    let target_year = (total_months / 12) as i32;
    let target_month = (total_months % 12) as u32 + 1;

    // Clamp day to last day of target month
    let max_day = days_in_month(target_year, target_month);
    let target_day = base.day().min(max_day);

    NaiveDate::from_ymd_opt(target_year, target_month, target_day)
        .ok_or_else(|| format!("Invalid result date: {target_year}-{target_month}-{target_day}"))
}

fn days_in_month(year: i32, month: u32) -> u32 {
    // Use the first day of the next month minus one day trick
    if month == 12 {
        NaiveDate::from_ymd_opt(year + 1, 1, 1)
    } else {
        NaiveDate::from_ymd_opt(year, month + 1, 1)
    }
    .unwrap()
    .pred_opt()
    .unwrap()
    .day()
}

// ---------------------------------------------------------------------------
// Formatting helpers
// ---------------------------------------------------------------------------

fn ordinal_suffix(day: u32) -> &'static str {
    match day {
        1 | 21 | 31 => "st",
        2 | 22 => "nd",
        3 | 23 => "rd",
        _ => "th",
    }
}

fn format_human_datetime<Tz: chrono::TimeZone>(dt: &chrono::DateTime<Tz>) -> String
where
    Tz::Offset: std::fmt::Display,
{
    let weekday = dt.format("%A");
    let month = dt.format("%B");
    let day = dt.day();
    let year = dt.year();
    let hour = dt.format("%-I");
    let minute = dt.format("%M");
    let ampm = dt.format("%p");

    format!(
        "{weekday}, {month} {day}{suffix}, {year} \u{2014} {hour}:{minute} {ampm}",
        suffix = ordinal_suffix(day),
    )
}

fn format_human_date<Tz: chrono::TimeZone>(dt: &chrono::DateTime<Tz>) -> String
where
    Tz::Offset: std::fmt::Display,
{
    let weekday = dt.format("%A");
    let month = dt.format("%B");
    let day = dt.day();
    let year = dt.year();

    format!(
        "{weekday}, {month} {day}{suffix}, {year}",
        suffix = ordinal_suffix(day),
    )
}

fn approximate_months(from: NaiveDate, to: NaiveDate) -> i64 {
    let year_diff = to.year() as i64 - from.year() as i64;
    let month_diff = to.month() as i64 - from.month() as i64;
    let total = year_diff * 12 + month_diff;
    // Adjust if the day hasn't been reached yet
    if total > 0 && to.day() < from.day() {
        total - 1
    } else if total < 0 && to.day() > from.day() {
        total + 1
    } else {
        total
    }
}

fn format_human_diff(days: i64) -> String {
    let abs_days = days.unsigned_abs();
    let prefix = if days < 0 { "minus " } else { "" };

    if abs_days == 0 {
        return "0 days".to_string();
    }

    let weeks = abs_days / 7;
    let remaining_days = abs_days % 7;
    let months = abs_days / 30;

    if abs_days < 7 {
        format!("{prefix}{abs_days} day{}", if abs_days == 1 { "" } else { "s" })
    } else if remaining_days == 0 {
        format!("{prefix}{weeks} week{}", if weeks == 1 { "" } else { "s" })
    } else if months >= 1 && abs_days.is_multiple_of(30) {
        format!("{prefix}{months} month{}", if months == 1 { "" } else { "s" })
    } else {
        format!("{prefix}{abs_days} days")
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn test_ctx() -> ToolContext {
        ToolContext::new("test-agent", "test")
    }

    #[test]
    fn should_have_correct_name_and_schema() {
        let tool = DatetimeTool::new();
        let def = tool.def();
        assert_eq!(def.name, "datetime");

        let schema = def.input_schema.clone();
        assert_eq!(schema["type"], "object");
        assert!(schema["properties"]["function"].is_object());
        assert!(schema["properties"]["timezone"].is_object());
        assert!(schema["properties"]["date"].is_object());
        assert!(schema["properties"]["offset"].is_object());
        assert!(schema["properties"]["from"].is_object());
        assert!(schema["properties"]["to"].is_object());
        assert_eq!(schema["required"], json!(["function"]));

        // Validate schema itself
        def.validate_input_schema().unwrap();
    }

    #[tokio::test]
    async fn get_current_datetime_returns_iso_string() {
        let tool = DatetimeTool::new();
        let ctx = test_ctx();
        let input = json!({"function": "get_current_datetime"});
        let result = tool.call(input, &ctx).await;

        assert!(!result.is_error);
        let content = &result.content[0];
        if let crate::ToolContent::Json(v) = content {
            assert!(v["iso"].is_string());
            assert!(v["date"].is_string());
            assert!(v["time"].is_string());
            assert!(v["weekday"].is_string());
            assert!(v["human"].is_string());
            // ISO string should not be empty
            assert!(!v["iso"].as_str().unwrap().is_empty());
        } else {
            panic!("Expected JSON content");
        }
    }

    #[tokio::test]
    async fn get_current_datetime_with_timezone() {
        let tool = DatetimeTool::new();
        let ctx = test_ctx();
        let input = json!({"function": "get_current_datetime", "timezone": "Asia/Taipei"});
        let result = tool.call(input, &ctx).await;

        assert!(!result.is_error);
        let content = &result.content[0];
        if let crate::ToolContent::Json(v) = content {
            let iso = v["iso"].as_str().unwrap();
            // Asia/Taipei is UTC+8
            assert!(iso.contains("+08:00"));
        } else {
            panic!("Expected JSON content");
        }
    }

    #[tokio::test]
    async fn date_add_plus_one_day() {
        let tool = DatetimeTool::new();
        let ctx = test_ctx();
        let input = json!({
            "function": "date_add",
            "date": "2026-03-17",
            "offset": "+1d"
        });
        let result = tool.call(input, &ctx).await;

        assert!(!result.is_error);
        let content = &result.content[0];
        if let crate::ToolContent::Json(v) = content {
            assert_eq!(v["date"].as_str().unwrap(), "2026-03-18");
            assert_eq!(v["weekday"].as_str().unwrap(), "Wednesday");
        } else {
            panic!("Expected JSON content");
        }
    }

    #[tokio::test]
    async fn date_add_plus_two_weeks() {
        let tool = DatetimeTool::new();
        let ctx = test_ctx();
        let input = json!({
            "function": "date_add",
            "date": "2026-03-17",
            "offset": "+2w"
        });
        let result = tool.call(input, &ctx).await;

        assert!(!result.is_error);
        let content = &result.content[0];
        if let crate::ToolContent::Json(v) = content {
            assert_eq!(v["date"].as_str().unwrap(), "2026-03-31");
        } else {
            panic!("Expected JSON content");
        }
    }

    #[tokio::test]
    async fn date_add_next_monday() {
        let tool = DatetimeTool::new();
        let ctx = test_ctx();
        // 2026-03-17 is a Tuesday
        let input = json!({
            "function": "date_add",
            "date": "2026-03-17",
            "offset": "next monday"
        });
        let result = tool.call(input, &ctx).await;

        assert!(!result.is_error);
        let content = &result.content[0];
        if let crate::ToolContent::Json(v) = content {
            assert_eq!(v["date"].as_str().unwrap(), "2026-03-23");
            assert_eq!(v["weekday"].as_str().unwrap(), "Monday");
        } else {
            panic!("Expected JSON content");
        }
    }

    #[tokio::test]
    async fn date_diff_two_weeks() {
        let tool = DatetimeTool::new();
        let ctx = test_ctx();
        let input = json!({
            "function": "date_diff",
            "from": "2026-03-17",
            "to": "2026-03-31"
        });
        let result = tool.call(input, &ctx).await;

        assert!(!result.is_error);
        let content = &result.content[0];
        if let crate::ToolContent::Json(v) = content {
            assert_eq!(v["days"].as_i64().unwrap(), 14);
            assert_eq!(v["weeks"].as_u64().unwrap(), 2);
            assert_eq!(v["months"].as_i64().unwrap(), 0);
            assert_eq!(v["human"].as_str().unwrap(), "2 weeks");
        } else {
            panic!("Expected JSON content");
        }
    }

    #[tokio::test]
    async fn unknown_function_returns_error() {
        let tool = DatetimeTool::new();
        let ctx = test_ctx();
        let input = json!({"function": "not_a_function"});
        let result = tool.call(input, &ctx).await;

        assert!(result.is_error);
        assert!(result.as_text().unwrap().contains("Unknown function"));
    }

    #[tokio::test]
    async fn invalid_input_returns_error() {
        let tool = DatetimeTool::new();
        let ctx = test_ctx();
        let input = json!({"not_function": "test"});
        let result = tool.call(input, &ctx).await;

        assert!(result.is_error);
        assert!(result.as_text().unwrap().contains("Invalid input"));
    }

    #[tokio::test]
    async fn date_add_missing_date_returns_error() {
        let tool = DatetimeTool::new();
        let ctx = test_ctx();
        let input = json!({"function": "date_add", "offset": "+1d"});
        let result = tool.call(input, &ctx).await;

        assert!(result.is_error);
    }

    #[tokio::test]
    async fn date_add_plus_one_month() {
        let tool = DatetimeTool::new();
        let ctx = test_ctx();
        let input = json!({
            "function": "date_add",
            "date": "2026-01-31",
            "offset": "+1M"
        });
        let result = tool.call(input, &ctx).await;

        assert!(!result.is_error);
        let content = &result.content[0];
        if let crate::ToolContent::Json(v) = content {
            // Jan 31 + 1M = Feb 28 (clamped)
            assert_eq!(v["date"].as_str().unwrap(), "2026-02-28");
        } else {
            panic!("Expected JSON content");
        }
    }

    #[test]
    fn parse_offset_negative_days() {
        let base = NaiveDate::from_ymd_opt(2026, 3, 17).unwrap();
        let result = parse_offset(base, "-7d").unwrap();
        assert_eq!(result, NaiveDate::from_ymd_opt(2026, 3, 10).unwrap());
    }

    #[test]
    fn parse_offset_invalid_returns_error() {
        let base = NaiveDate::from_ymd_opt(2026, 3, 17).unwrap();
        assert!(parse_offset(base, "garbage").is_err());
        assert!(parse_offset(base, "+1x").is_err());
    }
}
