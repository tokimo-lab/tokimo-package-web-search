//! 引擎之间共享的小工具：HTML 文本抽取 / URL 规范化 / 日期解析 等。

use std::sync::LazyLock;

use chrono::{DateTime, NaiveDate, TimeZone, Utc};
use regex::Regex;
use scraper::{ElementRef, Selector};

/// 仿 searxng `extract_text`：把一个节点下所有可见文本收集成一行，压掉连续空白。
pub fn extract_text(el: &ElementRef<'_>) -> String {
    let raw: String = el.text().collect::<Vec<_>>().join(" ");
    collapse_whitespace(&raw)
}

#[allow(dead_code)]
pub fn extract_text_from_sel(root: &ElementRef<'_>, sel: &Selector) -> String {
    root.select(sel)
        .next()
        .map(|e| extract_text(&e))
        .unwrap_or_default()
}

pub fn collapse_whitespace(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut prev_space = true;
    for c in s.chars() {
        if c.is_whitespace() {
            if !prev_space {
                out.push(' ');
                prev_space = true;
            }
        } else {
            out.push(c);
            prev_space = false;
        }
    }
    out.trim().to_string()
}

/// 去掉 HTML 实体（&amp; 等）
pub fn html_unescape(s: &str) -> String {
    html_escape::decode_html_entities(s).into_owned()
}

/// 把裸 HTML 字符串压成纯文本
pub fn html_to_text(s: &str) -> String {
    if !s.contains('<') {
        return collapse_whitespace(&html_unescape(s));
    }
    // 简单去 tag：scraper 需要完整 document，加 wrapper
    let wrapped = format!("<div>{s}</div>");
    let frag = scraper::Html::parse_fragment(&wrapped);
    let root = frag.root_element();
    extract_text(&root)
}

// ---------------------------------------------------------------------------
// Date parsing
// ---------------------------------------------------------------------------

/// Convert a Unix timestamp to `DateTime<Utc>`.
#[allow(dead_code)]
pub fn unix_ts(ts: i64) -> Option<DateTime<Utc>> {
    Utc.timestamp_opt(ts, 0).single()
}

/// Try to extract a date/time from a free-text string.
///
/// Handles ISO, English month names, Chinese dates, and relative expressions
/// in both Chinese and English. Returns `None` if nothing matches.
pub fn parse_date_text(raw: &str) -> Option<DateTime<Utc>> {
    let s = strip_prefix_noise(raw);
    let s = strip_suffix_noise(&s);

    // Try absolute formats first, then relative.
    parse_iso(&s)
        .or_else(|| parse_english_month(&s))
        .or_else(|| parse_chinese_date(&s))
        .or_else(|| parse_slash_date(&s))
        .or_else(|| parse_relative_chinese(&s))
        .or_else(|| parse_relative_english(&s))
}

// ---- noise stripping ------------------------------------------------------

fn strip_prefix_noise(s: &str) -> String {
    static RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?i)^(发帖时间|发贴时间|上传时间|发布时间|发布于|发表于|更新时间|published|posted|updated|created|date)[：:\s]*")
            .unwrap()
    });
    RE.replace(s, "").into_owned()
}

fn strip_suffix_noise(s: &str) -> String {
    static RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"[\s\-—·|,，。]+$").unwrap());
    RE.replace(s, "").into_owned()
}

// ---- ISO ------------------------------------------------------------------

fn parse_iso(s: &str) -> Option<DateTime<Utc>> {
    static RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(
            r"(\d{4})-(\d{1,2})-(\d{1,2})[T ](\d{1,2}):(\d{2})(?::(\d{2}))?(Z|[+-]\d{2}:?\d{2})?",
        )
        .unwrap()
    });
    if let Some(c) = RE.captures(s) {
        let y: i32 = c.get(1)?.as_str().parse().ok()?;
        let m: u32 = c.get(2)?.as_str().parse().ok()?;
        let d: u32 = c.get(3)?.as_str().parse().ok()?;
        let h: u32 = c.get(4)?.as_str().parse().ok()?;
        let min: u32 = c.get(5)?.as_str().parse().ok()?;
        let sec: u32 = c.get(6).and_then(|m| m.as_str().parse().ok()).unwrap_or(0);
        let ndt = NaiveDate::from_ymd_opt(y, m, d)?.and_hms_opt(h, min, sec)?;
        // Parse offset if present
        if let Some(offset) = c.get(7) {
            let off_str = offset.as_str();
            if off_str == "Z" {
                return Some(ndt.and_utc());
            }
            // +08:00 or +0800
            let sign: i32 = if off_str.starts_with('+') { 1 } else { -1 };
            let digits: String = off_str[1..]
                .chars()
                .filter(|c| c.is_ascii_digit())
                .collect();
            if digits.len() >= 4 {
                let oh: i32 = digits[..2].parse().ok()?;
                let om: i32 = digits[2..4].parse().ok()?;
                let offset_secs = sign * (oh * 3600 + om * 60);
                return Utc
                    .timestamp_opt(ndt.and_utc().timestamp() - i64::from(offset_secs), 0)
                    .single();
            }
        }
        return Some(ndt.and_utc());
    }
    // date-only: 2026-04-16 or 2026/04/16
    static RE_DATE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"^(\d{4})[-/](\d{1,2})[-/](\d{1,2})$").unwrap());
    if let Some(c) = RE_DATE.captures(s) {
        let y: i32 = c.get(1)?.as_str().parse().ok()?;
        let m: u32 = c.get(2)?.as_str().parse().ok()?;
        let d: u32 = c.get(3)?.as_str().parse().ok()?;
        let nd = NaiveDate::from_ymd_opt(y, m, d)?;
        return Some(nd.and_hms_opt(0, 0, 0)?.and_utc());
    }
    None
}

// ---- English month names --------------------------------------------------

const MONTHS: &[(&str, u32)] = &[
    ("january", 1),
    ("jan", 1),
    ("february", 2),
    ("feb", 2),
    ("march", 3),
    ("mar", 3),
    ("april", 4),
    ("apr", 4),
    ("may", 5),
    ("june", 6),
    ("jun", 6),
    ("july", 7),
    ("jul", 7),
    ("august", 8),
    ("aug", 8),
    ("september", 9),
    ("sep", 9),
    ("october", 10),
    ("oct", 10),
    ("november", 11),
    ("nov", 11),
    ("december", 12),
    ("dec", 12),
];

fn month_from_name(name: &str) -> Option<u32> {
    let lower = name.to_lowercase();
    MONTHS.iter().find(|(n, _)| *n == lower).map(|(_, m)| *m)
}

fn parse_english_month(s: &str) -> Option<DateTime<Utc>> {
    // "May 15, 2026" / "May 15th, 2026" / "May 15 2026"
    // "15 May 2026" / "15th May 2026" / "15 May, 2026"
    // "Thu, 16 Apr 2026"
    static RE_MDY: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?i)(?:[A-Za-z]{3},\s*)?([A-Za-z]+)\s+(\d{1,2})(?:st|nd|rd|th)?,?\s+(\d{4})")
            .unwrap()
    });
    static RE_DMY: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?i)(\d{1,2})(?:st|nd|rd|th)?\s+([A-Za-z]+),?\s+(\d{4})").unwrap()
    });

    if let Some(c) = RE_MDY.captures(s) {
        let m = month_from_name(c.get(1)?.as_str())?;
        let d: u32 = c.get(2)?.as_str().parse().ok()?;
        let y: i32 = c.get(3)?.as_str().parse().ok()?;
        let nd = NaiveDate::from_ymd_opt(y, m, d)?;
        return Some(nd.and_hms_opt(0, 0, 0)?.and_utc());
    }
    if let Some(c) = RE_DMY.captures(s) {
        let d: u32 = c.get(1)?.as_str().parse().ok()?;
        let m = month_from_name(c.get(2)?.as_str())?;
        let y: i32 = c.get(3)?.as_str().parse().ok()?;
        let nd = NaiveDate::from_ymd_opt(y, m, d)?;
        return Some(nd.and_hms_opt(0, 0, 0)?.and_utc());
    }
    None
}

// ---- Chinese dates --------------------------------------------------------

fn parse_chinese_date(s: &str) -> Option<DateTime<Utc>> {
    // "2026年4月16日" / "2026年04月16日"
    static RE_FULL: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"(\d{4})年(\d{1,2})月(\d{1,2})日?").unwrap());
    // "2026年4月" (no day) — match year+month, then check no digit follows 月
    static RE_YM: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(\d{4})年(\d{1,2})月").unwrap());

    if let Some(c) = RE_FULL.captures(s) {
        let y: i32 = c.get(1)?.as_str().parse().ok()?;
        let m: u32 = c.get(2)?.as_str().parse().ok()?;
        let d: u32 = c.get(3)?.as_str().parse().ok()?;
        let nd = NaiveDate::from_ymd_opt(y, m, d)?;
        return Some(nd.and_hms_opt(0, 0, 0)?.and_utc());
    }
    if let Some(c) = RE_YM.captures(s) {
        // Only match if no digit follows 月 (i.e. year-month only, not year-month-day)
        let end = c.get(0)?.end();
        if s[end..].starts_with(|ch: char| ch.is_ascii_digit()) {
            return None;
        }
        let y: i32 = c.get(1)?.as_str().parse().ok()?;
        let m: u32 = c.get(2)?.as_str().parse().ok()?;
        let nd = NaiveDate::from_ymd_opt(y, m, 1)?;
        return Some(nd.and_hms_opt(0, 0, 0)?.and_utc());
    }
    None
}

// ---- Ambiguous slash dates (MM/DD/YYYY) -----------------------------------

fn parse_slash_date(s: &str) -> Option<DateTime<Utc>> {
    static RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"^(\d{1,2})/(\d{1,2})/(\d{4})$").unwrap());
    if let Some(c) = RE.captures(s) {
        let a: u32 = c.get(1)?.as_str().parse().ok()?;
        let b: u32 = c.get(2)?.as_str().parse().ok()?;
        let y: i32 = c.get(3)?.as_str().parse().ok()?;
        // Heuristic: if first > 12 it's DD/MM/YYYY, else MM/DD/YYYY
        let (m, d) = if a > 12 { (b, a) } else { (a, b) };
        let nd = NaiveDate::from_ymd_opt(y, m, d)?;
        return Some(nd.and_hms_opt(0, 0, 0)?.and_utc());
    }
    None
}

// ---- Relative: Chinese ----------------------------------------------------

fn parse_relative_chinese(s: &str) -> Option<DateTime<Utc>> {
    static RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"(\d+)\s*(秒|分钟|小时|天|周|星期|个?月|年)前").unwrap());
    let now = Utc::now();
    if let Some(c) = RE.captures(s) {
        let n: i64 = c.get(1)?.as_str().parse().ok()?;
        let unit = c.get(2)?.as_str();
        return Some(sub_unit(now, n, unit));
    }
    // Fixed words
    match s {
        "今天" => Some(now),
        "昨天" => Some(now - chrono::Duration::days(1)),
        "前天" => Some(now - chrono::Duration::days(2)),
        _ => None,
    }
}

// ---- Relative: English ----------------------------------------------------

fn parse_relative_english(s: &str) -> Option<DateTime<Utc>> {
    static RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?i)(\d+|an?)\s*(second|minute|hour|day|week|month|year)s?\s*ago").unwrap()
    });
    let now = Utc::now();
    if let Some(c) = RE.captures(s) {
        let num_str = c.get(1)?.as_str().to_lowercase();
        let n: i64 = if num_str == "an" || num_str == "a" {
            1
        } else {
            num_str.parse().ok()?
        };
        let unit = c.get(2)?.as_str().to_lowercase();
        return Some(sub_unit(now, n, &unit));
    }
    match s.to_lowercase().as_str() {
        "just now" | "now" => Some(now),
        "yesterday" => Some(now - chrono::Duration::days(1)),
        _ => None,
    }
}

fn sub_unit(now: DateTime<Utc>, n: i64, unit: &str) -> DateTime<Utc> {
    let dur = match unit {
        "秒" | "second" => chrono::Duration::seconds(n),
        "分钟" | "minute" => chrono::Duration::minutes(n),
        "小时" | "hour" => chrono::Duration::hours(n),
        "天" | "day" => chrono::Duration::days(n),
        "周" | "星期" | "week" => chrono::Duration::weeks(n),
        "月" | "个月" | "month" => chrono::Duration::days(n * 30),
        "年" | "year" => chrono::Duration::days(n * 365),
        _ => return now,
    };
    now - dur
}

/// 组合 selector 构造快捷宏
#[macro_export]
macro_rules! sel {
    ($e:expr) => {
        ::scraper::Selector::parse($e).expect("invalid selector")
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    // ---- ISO --------------------------------------------------------------

    #[test]
    fn iso_datetime_utc() {
        let d = parse_date_text("2026-04-16T10:30:00Z").unwrap();
        assert_eq!(
            d.format("%Y-%m-%d %H:%M:%S").to_string(),
            "2026-04-16 10:30:00"
        );
    }

    #[test]
    fn iso_datetime_offset() {
        let d = parse_date_text("2026-04-16T18:30:00+08:00").unwrap();
        assert_eq!(
            d.format("%Y-%m-%d %H:%M:%S").to_string(),
            "2026-04-16 10:30:00"
        );
    }

    #[test]
    fn iso_datetime_space() {
        let d = parse_date_text("2023-10-08 20:21:00").unwrap();
        assert_eq!(d.format("%Y-%m-%d %H:%M").to_string(), "2023-10-08 20:21");
    }

    #[test]
    fn iso_date_dash() {
        let d = parse_date_text("2026-04-16").unwrap();
        assert_eq!(d.format("%Y-%m-%d").to_string(), "2026-04-16");
    }

    #[test]
    fn iso_date_slash() {
        let d = parse_date_text("2026/04/16").unwrap();
        assert_eq!(d.format("%Y-%m-%d").to_string(), "2026-04-16");
    }

    // ---- English month names -----------------------------------------------

    #[test]
    fn eng_mdy_comma() {
        let d = parse_date_text("May 15, 2026").unwrap();
        assert_eq!(d.format("%Y-%m-%d").to_string(), "2026-05-15");
    }

    #[test]
    fn eng_mdy_ordinal() {
        let d = parse_date_text("May 15th, 2026").unwrap();
        assert_eq!(d.format("%Y-%m-%d").to_string(), "2026-05-15");
    }

    #[test]
    fn eng_mdy_no_comma() {
        let d = parse_date_text("May 15 2026").unwrap();
        assert_eq!(d.format("%Y-%m-%d").to_string(), "2026-05-15");
    }

    #[test]
    fn eng_dmy() {
        let d = parse_date_text("15 May 2026").unwrap();
        assert_eq!(d.format("%Y-%m-%d").to_string(), "2026-05-15");
    }

    #[test]
    fn eng_dmy_ordinal() {
        let d = parse_date_text("16th April 2026").unwrap();
        assert_eq!(d.format("%Y-%m-%d").to_string(), "2026-04-16");
    }

    #[test]
    fn eng_rfc2822ish() {
        let d = parse_date_text("Thu, 16 Apr 2026").unwrap();
        assert_eq!(d.format("%Y-%m-%d").to_string(), "2026-04-16");
    }

    #[test]
    fn eng_abbrev_month() {
        let d = parse_date_text("Jan 5, 2023").unwrap();
        assert_eq!(d.format("%Y-%m-%d").to_string(), "2023-01-05");
    }

    #[test]
    fn eng_case_insensitive() {
        let d = parse_date_text("january 1, 2020").unwrap();
        assert_eq!(d.format("%Y-%m-%d").to_string(), "2020-01-01");
    }

    // ---- Chinese dates ----------------------------------------------------

    #[test]
    fn cn_full_date() {
        let d = parse_date_text("2026年4月16日").unwrap();
        assert_eq!(d.format("%Y-%m-%d").to_string(), "2026-04-16");
    }

    #[test]
    fn cn_full_date_no_day_char() {
        let d = parse_date_text("2026年4月16").unwrap();
        assert_eq!(d.format("%Y-%m-%d").to_string(), "2026-04-16");
    }

    #[test]
    fn cn_year_month_only() {
        let d = parse_date_text("2026年4月").unwrap();
        assert_eq!(d.format("%Y-%m-%d").to_string(), "2026-04-01");
    }

    #[test]
    fn cn_padded() {
        let d = parse_date_text("2026年04月06日").unwrap();
        assert_eq!(d.format("%Y-%m-%d").to_string(), "2026-04-06");
    }

    // ---- Prefix / suffix stripping ----------------------------------------

    #[test]
    fn cn_with_prefix() {
        let d = parse_date_text("发贴时间：2025年3月31日").unwrap();
        assert_eq!(d.format("%Y-%m-%d").to_string(), "2025-03-31");
    }

    #[test]
    fn cn_upload_prefix() {
        let d = parse_date_text("上传时间: 2015年6月15日").unwrap();
        assert_eq!(d.format("%Y-%m-%d").to_string(), "2015-06-15");
    }

    #[test]
    fn cn_trailing_dash() {
        let d = parse_date_text("2026年4月16日-").unwrap();
        assert_eq!(d.format("%Y-%m-%d").to_string(), "2026-04-16");
    }

    #[test]
    fn cn_trailing_dash_relative() {
        let d = parse_date_text("22小时前-");
        assert!(d.is_some());
    }

    // ---- Relative: Chinese ------------------------------------------------

    #[test]
    fn cn_relative_hours() {
        let d = parse_date_text("3小时前").unwrap();
        let now = Utc::now();
        assert!((now - d).num_hours() >= 2 && (now - d).num_hours() <= 4);
    }

    #[test]
    fn cn_relative_days() {
        let d = parse_date_text("2天前").unwrap();
        let now = Utc::now();
        assert!((now - d).num_days() >= 1 && (now - d).num_days() <= 3);
    }

    #[test]
    fn cn_relative_minutes() {
        let d = parse_date_text("30分钟前").unwrap();
        let now = Utc::now();
        assert!((now - d).num_minutes() >= 29 && (now - d).num_minutes() <= 31);
    }

    #[test]
    fn cn_relative_years() {
        let d = parse_date_text("2年前").unwrap();
        let now = Utc::now();
        assert!((now - d).num_days() >= 729 && (now - d).num_days() <= 731);
    }

    #[test]
    fn cn_yesterday() {
        let d = parse_date_text("昨天").unwrap();
        let now = Utc::now();
        assert!((now - d).num_days() >= 0 && (now - d).num_days() <= 2);
    }

    // ---- Relative: English ------------------------------------------------

    #[test]
    fn eng_relative_hours() {
        let d = parse_date_text("3 hours ago").unwrap();
        let now = Utc::now();
        assert!((now - d).num_hours() >= 2 && (now - d).num_hours() <= 4);
    }

    #[test]
    fn eng_relative_days() {
        let d = parse_date_text("2 days ago").unwrap();
        let now = Utc::now();
        assert!((now - d).num_days() >= 1 && (now - d).num_days() <= 3);
    }

    #[test]
    fn eng_relative_an_hour() {
        let d = parse_date_text("an hour ago").unwrap();
        let now = Utc::now();
        assert!((now - d).num_minutes() >= 59 && (now - d).num_minutes() <= 61);
    }

    #[test]
    fn eng_relative_a_day() {
        let d = parse_date_text("a day ago").unwrap();
        let now = Utc::now();
        assert!((now - d).num_hours() >= 23 && (now - d).num_hours() <= 25);
    }

    #[test]
    fn eng_yesterday() {
        let d = parse_date_text("yesterday").unwrap();
        let now = Utc::now();
        assert!((now - d).num_days() >= 0 && (now - d).num_days() <= 2);
    }

    #[test]
    fn eng_just_now() {
        let d = parse_date_text("just now").unwrap();
        let now = Utc::now();
        assert!((now - d).num_seconds() < 5);
    }

    // ---- Slash date (ambiguous) -------------------------------------------

    #[test]
    fn slash_mmddyyyy() {
        // 13/04/2026 → DD/MM (a > 12)
        let d = parse_date_text("13/04/2026").unwrap();
        assert_eq!(d.format("%Y-%m-%d").to_string(), "2026-04-13");
    }

    #[test]
    fn slash_ambiguous_us() {
        // 04/16/2026 → MM/DD (a <= 12, b > 12 → impossible as DD, so MM/DD)
        // Actually a=4, b=16 → a<=12 so (a,b)=(4,16) → April 16
        let d = parse_date_text("04/16/2026").unwrap();
        assert_eq!(d.format("%Y-%m-%d").to_string(), "2026-04-16");
    }

    // ---- Edge cases -------------------------------------------------------

    #[test]
    fn empty_string() {
        assert!(parse_date_text("").is_none());
    }

    #[test]
    fn garbage() {
        assert!(parse_date_text("no date here").is_none());
    }

    #[test]
    fn bing_result_style() {
        let d = parse_date_text("2026年4月16日 Rust 程序设计语言中文也译为 Rust 权威指南").unwrap();
        assert_eq!(d.format("%Y-%m-%d").to_string(), "2026-04-16");
    }
}
