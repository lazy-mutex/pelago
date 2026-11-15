// Pure-Rust civil date/time math and the parts of dayjs 1.11 that Mermaid's gantt code relies on,
// with the plugins it loads (custom parse format, advanced format, ISO week) and the English locale.

use crate::chars::is_space;
use crate::svg::builder::js_round;

pub const MONTHS: [&str; 12] = [
    "January",
    "February",
    "March",
    "April",
    "May",
    "June",
    "July",
    "August",
    "September",
    "October",
    "November",
    "December",
];

pub const WEEKDAYS: [&str; 7] = [
    "Sunday",
    "Monday",
    "Tuesday",
    "Wednesday",
    "Thursday",
    "Friday",
    "Saturday",
];

const DAY_MS: i128 = 86_400_000;
const DAY_F64: f64 = 86_400_000.0;
const WEEK_F64: f64 = 7.0 * DAY_F64;
const MAX_TIME_MS: i128 = 8_640_000_000_000_000;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Unit {
    Ms,
    S,
    Min,
    H,
    D,
    W,
    Month,
    Y,
}

#[derive(Debug, Clone, Copy)]
pub struct JsDate(pub f64);

impl PartialEq for JsDate {
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}

#[derive(Debug, Clone, Copy)]
struct Parts {
    year: i64,
    month: i64, // 0..=11
    day: i64,   // 1..=31
    weekday: i64, // 0 (Sunday)..=6 (Saturday)
    hours: i64,
    minutes: i64,
    seconds: i64,
    millis: i64,
}

// Days since 1970-01-01 in the proleptic Gregorian calendar (`m` in 1..=12, `d` >= 1).
fn days_from_civil(y: i128, m: i128, d: i128) -> i128 {
    let y = y - if m <= 2 { 1 } else { 0 };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let doy = (153 * (m + if m > 2 { -3 } else { 9 }) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146097 + doe - 719468
}

// Civil `(year, month_0_based, day_1_based)` from days since 1970-01-01.
fn civil_from_days(z: i64) -> (i64, i64, i64) {
    let z = z + 719468;
    let era = z.div_euclid(146097);
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = mp + if mp < 10 { 3 } else { -9 };
    let y = y + if m <= 2 { 1 } else { 0 };
    (y, m - 1, d)
}

fn time_clip(ms: f64) -> JsDate {
    if !ms.is_finite() || ms.abs() > MAX_TIME_MS as f64 {
        return JsDate(f64::NAN);
    }
    let t = ms.trunc();
    JsDate(if t == 0.0 { 0.0 } else { t })
}

fn make_timestamp(
    year: i128,
    month_0: i128,
    day_1: i128,
    hours: i128,
    minutes: i128,
    seconds: i128,
    millis: i128,
) -> JsDate {
    if year.abs() > 1_000_000 || month_0.abs() > 12_000_000 || day_1.abs() > 400_000_000 {
        return JsDate(f64::NAN);
    }
    let ym = year + month_0.div_euclid(12);
    let mn = month_0.rem_euclid(12);
    let days = days_from_civil(ym, mn + 1, 1) + (day_1 - 1);
    let total = days * DAY_MS + hours * 3_600_000 + minutes * 60_000 + seconds * 1_000 + millis;
    if total.abs() > MAX_TIME_MS {
        JsDate(f64::NAN)
    } else {
        let f = total as f64;
        JsDate(if f == 0.0 { 0.0 } else { f })
    }
}

fn civil_days_in_month(year: i64, month_1: i64) -> i64 {
    match month_1 {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 => {
            let leap = (year.rem_euclid(4) == 0 && year.rem_euclid(100) != 0) || year.rem_euclid(400) == 0;
            if leap { 29 } else { 28 }
        }
        _ => 0,
    }
}

impl JsDate {
    pub const NAN: JsDate = JsDate(f64::NAN);

    pub fn from_ms(ms: f64) -> Self {
        time_clip(ms)
    }

    // Matches `new Date(year, month, day, h, m, s, ms)` / `Date.UTC(...)` (maps years 0..=99 to 1900 + year).
    pub fn from_ymd_hms_ms(
        year: i64,
        month_0: i64,
        day_1: i64,
        hours: i64,
        minutes: i64,
        seconds: i64,
        millis: i64,
    ) -> Self {
        let yr = if (0..=99).contains(&year) { 1900 + year } else { year };
        make_timestamp(
            yr as i128,
            month_0 as i128,
            day_1 as i128,
            hours as i128,
            minutes as i128,
            seconds as i128,
            millis as i128,
        )
    }

    // Exact calendar year without ECMAScript's 0..=99 -> 1900..=1999 offset (used by ISO 8601 & setFullYear).
    pub fn from_full_year(
        year: i64,
        month_0: i64,
        day_1: i64,
        hours: i64,
        minutes: i64,
        seconds: i64,
        millis: i64,
    ) -> Self {
        make_timestamp(
            year as i128,
            month_0 as i128,
            day_1 as i128,
            hours as i128,
            minutes as i128,
            seconds as i128,
            millis as i128,
        )
    }

    pub fn is_valid(self) -> bool {
        self.0.is_finite()
    }

    pub fn time(self) -> f64 {
        self.0
    }

    fn parts(self) -> Option<Parts> {
        if !self.0.is_finite() {
            return None;
        }
        let t = self.0 as i64;
        let days = t.div_euclid(86_400_000);
        let tod = t.rem_euclid(86_400_000);
        let (year, month, day) = civil_from_days(days);
        let weekday = (days + 4).rem_euclid(7);
        Some(Parts {
            year,
            month,
            day,
            weekday,
            hours: tod / 3_600_000,
            minutes: (tod / 60_000) % 60,
            seconds: (tod / 1_000) % 60,
            millis: tod % 1_000,
        })
    }

    pub fn full_year(self) -> i64 {
        self.parts().map_or(0, |p| p.year)
    }

    pub fn month(self) -> i64 {
        self.parts().map_or(0, |p| p.month)
    }

    pub fn date(self) -> i64 {
        self.parts().map_or(0, |p| p.day)
    }

    pub fn day(self) -> i64 {
        self.parts().map_or(0, |p| p.weekday)
    }

    pub fn hours(self) -> i64 {
        self.parts().map_or(0, |p| p.hours)
    }

    pub fn minutes(self) -> i64 {
        self.parts().map_or(0, |p| p.minutes)
    }

    pub fn seconds(self) -> i64 {
        self.parts().map_or(0, |p| p.seconds)
    }

    pub fn milliseconds(self) -> i64 {
        self.parts().map_or(0, |p| p.millis)
    }

    pub fn timezone_offset(self) -> i64 {
        0
    }

    pub fn set_time(&mut self, ms: f64) {
        *self = time_clip(ms);
    }

    pub fn set_hours(&mut self, h: i64, m: i64, s: i64, ms: i64) {
        let Some(p) = self.parts() else { return };
        *self = make_timestamp(
            p.year as i128,
            p.month as i128,
            p.day as i128,
            h as i128,
            m as i128,
            s as i128,
            ms as i128,
        );
    }

    pub fn set_date(&mut self, day_1: i64) {
        let Some(p) = self.parts() else { return };
        *self = make_timestamp(
            p.year as i128,
            p.month as i128,
            day_1 as i128,
            p.hours as i128,
            p.minutes as i128,
            p.seconds as i128,
            p.millis as i128,
        );
    }

    pub fn set_date_f64(&mut self, day_1: f64) {
        if !day_1.is_finite() || day_1.abs() > 1e12 {
            *self = JsDate::NAN;
            return;
        }
        self.set_date(day_1.trunc() as i64);
    }

    pub fn set_month(&mut self, month_0: i64) {
        let Some(p) = self.parts() else { return };
        *self = make_timestamp(
            p.year as i128,
            month_0 as i128,
            p.day as i128,
            p.hours as i128,
            p.minutes as i128,
            p.seconds as i128,
            p.millis as i128,
        );
    }

    pub fn set_month_f64(&mut self, month_0: f64) {
        if !month_0.is_finite() || month_0.abs() > 1e12 {
            *self = JsDate::NAN;
            return;
        }
        self.set_month(month_0.trunc() as i64);
    }

    pub fn set_month_day(&mut self, month_0: i64, day_1: i64) {
        let Some(p) = self.parts() else { return };
        *self = make_timestamp(
            p.year as i128,
            month_0 as i128,
            day_1 as i128,
            p.hours as i128,
            p.minutes as i128,
            p.seconds as i128,
            p.millis as i128,
        );
    }

    pub fn set_full_year(&mut self, year: i64) {
        let p = self.parts().unwrap_or(Parts {
            year: 1970,
            month: 0,
            day: 1,
            weekday: 4,
            hours: 0,
            minutes: 0,
            seconds: 0,
            millis: 0,
        });
        *self = make_timestamp(
            year as i128,
            p.month as i128,
            p.day as i128,
            p.hours as i128,
            p.minutes as i128,
            p.seconds as i128,
            p.millis as i128,
        );
    }

    pub fn set_full_year_f64(&mut self, year: f64) {
        if !year.is_finite() || year.abs() > 1e12 {
            *self = JsDate::NAN;
            return;
        }
        self.set_full_year(year.trunc() as i64);
    }
}

pub fn is_valid(date: JsDate) -> bool {
    date.is_valid()
}

fn pad(value: i64, length: usize) -> String {
    let s = value.to_string();
    if s.len() >= length {
        s
    } else {
        format!("{}{}", "0".repeat(length - s.len()), s)
    }
}

fn ordinal(n: i64) -> String {
    let suffixes = ["th", "st", "nd", "rd"];
    let v = n.rem_euclid(100);
    let suffix = if v >= 20 {
        let idx = ((v - 20) % 10) as usize;
        if idx < suffixes.len() && idx != 0 {
            suffixes[idx]
        } else {
            suffixes[0]
        }
    } else if (v as usize) < suffixes.len() {
        suffixes[v as usize]
    } else {
        suffixes[0]
    };
    format!("{}{}", n, suffix)
}

pub fn iso_weekday(date: JsDate) -> i64 {
    let d = date.day();
    if d == 0 { 7 } else { d }
}

pub fn add_days(date: JsDate, days: i64) -> JsDate {
    let mut next = date;
    next.set_date(next.date().saturating_add(days));
    next
}

fn days_in_month(date: JsDate) -> i64 {
    let mut last = JsDate::from_ymd_hms_ms(date.full_year(), date.month() + 1, 0, 0, 0, 0, 0);
    last.set_hours(23, 59, 59, 999);
    last.date()
}

// Sets the month or the year, keeping the day of the month unless the new month is shorter.
fn set_calendar(date: JsDate, apply: impl FnOnce(&mut JsDate)) -> JsDate {
    let mut next = date;
    next.set_date(1);
    apply(&mut next);
    let dim = days_in_month(next);
    next.set_date(date.date().min(dim));
    next
}

pub fn add(date: JsDate, amount: f64, unit: Unit) -> JsDate {
    match unit {
        Unit::Month => set_calendar(date, |d| d.set_month_f64(date.month() as f64 + amount)),
        Unit::Y => set_calendar(date, |d| d.set_full_year_f64(date.full_year() as f64 + amount)),
        Unit::D => {
            let mut next = date;
            next.set_date_f64(next.date() as f64 + js_round(amount));
            next
        }
        Unit::W => {
            let mut next = date;
            next.set_date_f64(next.date() as f64 + js_round(7.0 * amount));
            next
        }
        Unit::H => JsDate::from_ms(date.time() + amount * 3_600_000.0),
        Unit::Min => JsDate::from_ms(date.time() + amount * 60_000.0),
        Unit::S => JsDate::from_ms(date.time() + amount * 1_000.0),
        Unit::Ms => JsDate::from_ms(date.time() + amount),
    }
}

fn week_thursday(date: JsDate) -> JsDate {
    add_days(date, 4 - iso_weekday(date))
}

fn iso_week(date: JsDate) -> i64 {
    let thursday = week_thursday(date);
    let jan1 = JsDate::from_ymd_hms_ms(thursday.full_year(), 0, 1, 0, 0, 0, 0);
    let weekday = iso_weekday(jan1);
    let first = add_days(jan1, 4 - weekday + if weekday > 4 { 7 } else { 0 });
    let weeks = (thursday.time() - first.time()) / WEEK_F64;
    let whole = if weeks < 0.0 {
        let c = weeks.ceil();
        if c == 0.0 { 0 } else { c as i64 }
    } else {
        weeks.floor() as i64
    };
    whole + 1
}

pub fn iso_date(date: JsDate) -> String {
    let year = date.full_year();
    let month = date.month() + 1;
    let day = date.date();
    let y_str = if (1000..=9999).contains(&year) {
        year.to_string()
    } else {
        pad(year, 4)
    };
    let m_sep = if month < 10 { "-0" } else { "-" };
    let d_sep = if day < 10 { "-0" } else { "-" };
    format!("{}{}{}{}{}", y_str, m_sep, month, d_sep, day)
}

const DEFAULT_FORMAT: &str = "YYYY-MM-DDTHH:mm:ssZ";

fn starts_chars(s: &[char], at: usize, lit: &str) -> bool {
    let mut i = at;
    for ch in lit.chars() {
        if s.get(i) != Some(&ch) {
            return false;
        }
        i += 1;
    }
    true
}

// First pass of `format`: `/\[([^\]]+)]|Q|wo|ww|w|WW|W|zzz|z|gggg|GGGG|Do|X|x|k{1,2}|S/g`
fn replace_advanced(str_in: &str, date: JsDate) -> Option<String> {
    let s: Vec<char> = str_in.chars().collect();
    let n = s.len();
    let mut out = String::with_capacity(str_in.len() + 8);
    let mut i = 0usize;
    let mut unsupported = false;

    while i < n {
        if s[i] == '[' {
            let mut j = i + 1;
            while j < n && s[j] != ']' {
                j += 1;
            }
            if j > i + 1 && j < n {
                out.extend(&s[i..=j]);
                i = j + 1;
                continue;
            }
        }

        let mut matched: Option<(&str, usize)> = None;
        for tok in [
            "gggg", "GGGG", "zzz", "wo", "ww", "WW", "Do", "kk", "Q", "w", "W", "z", "X", "x", "k", "S",
        ] {
            if starts_chars(&s, i, tok) {
                matched = Some((tok, tok.len()));
                break;
            }
        }

        if let Some((tok, len)) = matched {
            match tok {
                "Q" => out.push_str(&(((date.month() + 1 + 2) / 3).to_string())),
                "Do" => {
                    out.push('[');
                    out.push_str(&ordinal(date.date()));
                    out.push(']');
                }
                "GGGG" => out.push_str(&week_thursday(date).full_year().to_string()),
                "W" => out.push_str(&iso_week(date).to_string()),
                "WW" => out.push_str(&pad(iso_week(date), 2)),
                "k" => {
                    let h = date.hours();
                    out.push_str(&(if h == 0 { 24 } else { h }).to_string());
                }
                "kk" => {
                    let h = date.hours();
                    out.push_str(&pad(if h == 0 { 24 } else { h }, 2));
                }
                "X" => out.push_str(&((date.time() / 1000.0).floor() as i64).to_string()),
                "x" => out.push_str(&(date.time() as i64).to_string()),
                "gggg" | "wo" | "w" | "ww" | "z" | "zzz" => {
                    unsupported = true;
                    out.push_str(tok);
                }
                _ => out.push_str(tok),
            }
            i += len;
        } else {
            out.push(s[i]);
            i += 1;
        }
    }

    if unsupported { None } else { Some(out) }
}

// Second pass of `format`: `/\[([^\]]+)]|YYYY|YY|M{1,4}|D{1,2}|d{1,4}|H{1,2}|h{1,2}|a|A|m{1,2}|s{1,2}|Z{1,2}|SSS/g`
fn replace_format(advanced: &str, date: JsDate) -> String {
    let s: Vec<char> = advanced.chars().collect();
    let n = s.len();
    let mut out = String::with_capacity(advanced.len() + 8);
    let hours = date.hours();
    let mut i = 0usize;

    while i < n {
        if s[i] == '[' {
            let mut j = i + 1;
            while j < n && s[j] != ']' {
                j += 1;
            }
            if j > i + 1 && j < n {
                out.extend(&s[i + 1..j]);
                i = j + 1;
                continue;
            }
        }

        let mut matched: Option<(&str, usize)> = None;
        for tok in [
            "YYYY", "MMMM", "dddd", "MMM", "ddd", "SSS", "YY", "MM", "DD", "dd", "HH", "hh", "mm",
            "ss", "ZZ", "M", "D", "d", "H", "h", "a", "A", "m", "s", "Z",
        ] {
            if starts_chars(&s, i, tok) {
                matched = Some((tok, tok.len()));
                break;
            }
        }

        if let Some((tok, len)) = matched {
            match tok {
                "YY" => {
                    let y_str = date.full_year().to_string();
                    let chars: Vec<char> = y_str.chars().collect();
                    let skip = chars.len().saturating_sub(2);
                    out.extend(&chars[skip..]);
                }
                "YYYY" => out.push_str(&pad(date.full_year(), 4)),
                "M" => out.push_str(&(date.month() + 1).to_string()),
                "MM" => out.push_str(&pad(date.month() + 1, 2)),
                "MMM" => out.push_str(&MONTHS[date.month() as usize][..3]),
                "MMMM" => out.push_str(MONTHS[date.month() as usize]),
                "D" => out.push_str(&date.date().to_string()),
                "DD" => out.push_str(&pad(date.date(), 2)),
                "d" => out.push_str(&date.day().to_string()),
                "dd" => out.push_str(&WEEKDAYS[date.day() as usize][..2]),
                "ddd" => out.push_str(&WEEKDAYS[date.day() as usize][..3]),
                "dddd" => out.push_str(WEEKDAYS[date.day() as usize]),
                "H" => out.push_str(&hours.to_string()),
                "HH" => out.push_str(&pad(hours, 2)),
                "h" => {
                    let h12 = if hours % 12 == 0 { 12 } else { hours % 12 };
                    out.push_str(&h12.to_string());
                }
                "hh" => {
                    let h12 = if hours % 12 == 0 { 12 } else { hours % 12 };
                    out.push_str(&pad(h12, 2));
                }
                "a" => out.push_str(if hours < 12 { "am" } else { "pm" }),
                "A" => out.push_str(if hours < 12 { "AM" } else { "PM" }),
                "m" => out.push_str(&date.minutes().to_string()),
                "mm" => out.push_str(&pad(date.minutes(), 2)),
                "s" => out.push_str(&date.seconds().to_string()),
                "ss" => out.push_str(&pad(date.seconds(), 2)),
                "SSS" => out.push_str(&pad(date.milliseconds(), 3)),
                "Z" => out.push_str("+00:00"),
                _ => out.push_str("+0000"),
            }
            i += len;
        } else {
            out.push(s[i]);
            i += 1;
        }
    }

    out
}

pub fn format(date: JsDate, template: &str) -> Option<String> {
    if !is_valid(date) {
        return Some("Invalid Date".into());
    }
    let str_tpl = if template.is_empty() { DEFAULT_FORMAT } else { template };
    if str_tpl == "YYYY-MM-DD" {
        return Some(iso_date(date));
    }
    let advanced = replace_advanced(str_tpl, date)?;
    Some(replace_format(&advanced, date))
}

#[derive(Default)]
struct Fields {
    year: Option<i64>,
    month: Option<i64>,
    day: Option<i64>,
    day_given: bool,
    hours: Option<i64>,
    minutes: Option<i64>,
    seconds: Option<i64>,
    milliseconds: Option<i64>,
    week: Option<i64>,
    offset: Option<i64>,
    afternoon: Option<bool>,
}

#[derive(Clone, Copy)]
enum MatchKind {
    D1,
    D2,
    D3,
    D4,
    D1To2,
    Signed,
    Offset,
    Word,
}

#[derive(Clone, Copy)]
enum ReaderOp {
    UpperMeridiem,
    LowerMeridiem,
    Quarter,
    Ms1,
    Ms2,
    Ms3,
    Seconds,
    Minutes,
    Hours,
    Day,
    DayOrdinal,
    Week,
    MonthNum,
    MonthShort,
    MonthLong,
    YearSigned,
    Year2,
    Year4,
    Offset,
}

enum Step {
    Lit(usize),
    Token(MatchKind, ReaderOp),
}

fn is_dayjs_sep(c: char) -> bool {
    matches!(c, '-' | '_' | ':' | '/' | '.' | ',' | '(' | ')') || is_space(c)
}

fn is_dayjs_word_char(c: char) -> bool {
    !matches!(c, '-' | '_' | ':' | '/' | ',' | '(' | ')') && !is_space(c) && !c.is_ascii_digit()
}

fn has_localized_token(template: &str) -> bool {
    let s: Vec<char> = template.chars().collect();
    let n = s.len();
    let mut i = 0;
    while i < n {
        if s[i] == '[' {
            let mut j = i + 1;
            while j < n && s[j] != ']' {
                j += 1;
            }
            if j > i + 1 && j < n {
                i = j + 1;
                continue;
            }
        }
        if s[i] == 'L' || s[i] == 'l' {
            return true;
        }
        i += 1;
    }
    false
}

fn reader_for(tok: &str) -> Option<(MatchKind, ReaderOp)> {
    Some(match tok {
        "A" => (MatchKind::Word, ReaderOp::UpperMeridiem),
        "a" => (MatchKind::Word, ReaderOp::LowerMeridiem),
        "Q" => (MatchKind::D1, ReaderOp::Quarter),
        "S" => (MatchKind::D1, ReaderOp::Ms1),
        "SS" => (MatchKind::D2, ReaderOp::Ms2),
        "SSS" => (MatchKind::D3, ReaderOp::Ms3),
        "s" | "ss" => (MatchKind::D1To2, ReaderOp::Seconds),
        "m" | "mm" => (MatchKind::D1To2, ReaderOp::Minutes),
        "H" | "h" | "HH" | "hh" => (MatchKind::D1To2, ReaderOp::Hours),
        "D" => (MatchKind::D1To2, ReaderOp::Day),
        "DD" => (MatchKind::D2, ReaderOp::Day),
        "Do" => (MatchKind::Word, ReaderOp::DayOrdinal),
        "w" => (MatchKind::D1To2, ReaderOp::Week),
        "ww" => (MatchKind::D2, ReaderOp::Week),
        "M" => (MatchKind::D1To2, ReaderOp::MonthNum),
        "MM" => (MatchKind::D2, ReaderOp::MonthNum),
        "MMM" => (MatchKind::Word, ReaderOp::MonthShort),
        "MMMM" => (MatchKind::Word, ReaderOp::MonthLong),
        "Y" => (MatchKind::Signed, ReaderOp::YearSigned),
        "YY" => (MatchKind::D2, ReaderOp::Year2),
        "YYYY" => (MatchKind::D4, ReaderOp::Year4),
        "Z" | "ZZ" => (MatchKind::Offset, ReaderOp::Offset),
        _ => return None,
    })
}

// Tokenizes `template` with `/(\[[^[]*\])|([-_:/.,()\s]+)|(A|a|Q|YYYY|YY?|ww?|MM?M?M?|Do|DD?|hh?|HH?|mm?|ss?|S{1,3}|z|ZZ?)/g`.
fn compile_steps(template: &str) -> Option<Vec<Step>> {
    if has_localized_token(template) {
        return None;
    }
    let s: Vec<char> = template.chars().collect();
    let n = s.len();
    let mut steps = Vec::new();
    let mut i = 0usize;

    while i < n {
        // Branch 1: `\[[^[]*\]`
        if s[i] == '[' {
            let mut j = i + 1;
            let mut found_close = None;
            while j < n && s[j] != '[' {
                if s[j] == ']' {
                    found_close = Some(j);
                    break;
                }
                j += 1;
            }
            if let Some(close) = found_close {
                // `token.replace(/^\[|\]$/g, '')` leaves `close - i - 1` characters.
                steps.push(Step::Lit(close - i - 1));
                i = close + 1;
                continue;
            }
        }

        // Branch 2: `[-_:/.,()\s]+`
        if is_dayjs_sep(s[i]) {
            let mut j = i + 1;
            while j < n && is_dayjs_sep(s[j]) {
                j += 1;
            }
            steps.push(Step::Lit(j - i));
            i = j;
            continue;
        }

        // Branch 3: `A|a|Q|YYYY|YY?|ww?|MM?M?M?|Do|DD?|hh?|HH?|mm?|ss?|S{1,3}|z|ZZ?`
        let mut tok_match: Option<&str> = None;
        for cand in [
            "YYYY", "MMMM", "MMM", "SSS", "YY", "ww", "MM", "Do", "DD", "hh", "HH", "mm", "ss",
            "SS", "ZZ", "A", "a", "Q", "Y", "w", "M", "D", "h", "H", "m", "s", "S", "z", "Z",
        ] {
            if starts_chars(&s, i, cand) {
                tok_match = Some(cand);
                break;
            }
        }

        if let Some(tok) = tok_match {
            if let Some((kind, op)) = reader_for(tok) {
                steps.push(Step::Token(kind, op));
            } else {
                steps.push(Step::Lit(tok.len()));
            }
            i += tok.len();
        } else {
            i += 1;
        }
    }

    if steps.is_empty() { None } else { Some(steps) }
}

fn find_match(kind: MatchKind, slice: &[char]) -> Option<(usize, usize)> {
    let n = slice.len();
    match kind {
        MatchKind::D1 => (0..n).find(|&i| slice[i].is_ascii_digit()).map(|i| (i, i + 1)),
        MatchKind::D2 => (0..n.saturating_sub(1))
            .find(|&i| slice[i].is_ascii_digit() && slice[i + 1].is_ascii_digit())
            .map(|i| (i, i + 2)),
        MatchKind::D3 => (0..n.saturating_sub(2))
            .find(|&i| {
                slice[i].is_ascii_digit()
                    && slice[i + 1].is_ascii_digit()
                    && slice[i + 2].is_ascii_digit()
            })
            .map(|i| (i, i + 3)),
        MatchKind::D4 => (0..n.saturating_sub(3))
            .find(|&i| {
                slice[i].is_ascii_digit()
                    && slice[i + 1].is_ascii_digit()
                    && slice[i + 2].is_ascii_digit()
                    && slice[i + 3].is_ascii_digit()
            })
            .map(|i| (i, i + 4)),
        MatchKind::D1To2 => {
            let i = (0..n).find(|&i| slice[i].is_ascii_digit())?;
            let end = if slice.get(i + 1).is_some_and(|c| c.is_ascii_digit()) {
                i + 2
            } else {
                i + 1
            };
            Some((i, end))
        }
        MatchKind::Signed => {
            for i in 0..n {
                let c = slice[i];
                if (c == '+' || c == '-') && slice.get(i + 1).is_some_and(|d| d.is_ascii_digit()) {
                    let mut j = i + 2;
                    while j < n && slice[j].is_ascii_digit() {
                        j += 1;
                    }
                    return Some((i, j));
                }
                if c.is_ascii_digit() {
                    let mut j = i + 1;
                    while j < n && slice[j].is_ascii_digit() {
                        j += 1;
                    }
                    return Some((i, j));
                }
            }
            None
        }
        MatchKind::Offset => {
            // `/[+-]\d\d:?(\d\d)?|Z/`
            for i in 0..n {
                let c = slice[i];
                if c == 'Z' {
                    return Some((i, i + 1));
                }
                if (c == '+' || c == '-')
                    && slice.get(i + 1).is_some_and(|d| d.is_ascii_digit())
                    && slice.get(i + 2).is_some_and(|d| d.is_ascii_digit())
                {
                    let mut j = i + 3;
                    if slice.get(j) == Some(&':') {
                        j += 1;
                    }
                    if slice.get(j).is_some_and(|d| d.is_ascii_digit())
                        && slice.get(j + 1).is_some_and(|d| d.is_ascii_digit())
                    {
                        j += 2;
                    }
                    return Some((i, j));
                }
            }
            None
        }
        MatchKind::Word => {
            // `/\d*[^-_:/,()\s\d]+/`
            for i in 0..n {
                let mut j = i;
                while j < n && slice[j].is_ascii_digit() {
                    j += 1;
                }
                if j < n && is_dayjs_word_char(slice[j]) {
                    while j < n && is_dayjs_word_char(slice[j]) {
                        j += 1;
                    }
                    return Some((i, j));
                }
            }
            None
        }
    }
}

fn apply_reader(op: ReaderOp, f: &mut Fields, matched: &str) -> bool {
    let parse_int = |s: &str| s.parse::<i64>().unwrap_or(0);
    match op {
        ReaderOp::UpperMeridiem => {
            f.afternoon = Some(matched == "PM");
            true
        }
        ReaderOp::LowerMeridiem => {
            f.afternoon = Some(matched == "pm");
            true
        }
        ReaderOp::Quarter => {
            f.month = Some((parse_int(matched) - 1) * 3 + 1);
            true
        }
        ReaderOp::Ms1 => {
            f.milliseconds = Some(parse_int(matched) * 100);
            true
        }
        ReaderOp::Ms2 => {
            f.milliseconds = Some(parse_int(matched) * 10);
            true
        }
        ReaderOp::Ms3 => {
            f.milliseconds = Some(parse_int(matched));
            true
        }
        ReaderOp::Seconds => {
            f.seconds = Some(parse_int(matched));
            true
        }
        ReaderOp::Minutes => {
            f.minutes = Some(parse_int(matched));
            true
        }
        ReaderOp::Hours => {
            f.hours = Some(parse_int(matched));
            true
        }
        ReaderOp::Day => {
            f.day = Some(parse_int(matched));
            true
        }
        ReaderOp::DayOrdinal => {
            let mut digits = String::new();
            let mut found = false;
            for c in matched.chars() {
                if c.is_ascii_digit() {
                    digits.push(c);
                    found = true;
                } else if found {
                    break;
                }
            }
            if !found {
                return false;
            }
            f.day = Some(parse_int(&digits));
            f.day_given = true;
            for d in 1..=31 {
                if ordinal(d) == matched {
                    f.day = Some(d);
                }
            }
            true
        }
        ReaderOp::Week => {
            f.week = Some(parse_int(matched));
            true
        }
        ReaderOp::MonthNum => {
            f.month = Some(parse_int(matched));
            true
        }
        ReaderOp::MonthShort => {
            let Some(idx) = MONTHS.iter().position(|m| &m[..3] == matched) else {
                return false;
            };
            let m1 = (idx + 1) as i64;
            f.month = Some(if m1 % 12 == 0 { m1 } else { m1 % 12 });
            true
        }
        ReaderOp::MonthLong => {
            let Some(idx) = MONTHS.iter().position(|&m| m == matched) else {
                return false;
            };
            let m1 = (idx + 1) as i64;
            f.month = Some(if m1 % 12 == 0 { m1 } else { m1 % 12 });
            true
        }
        ReaderOp::YearSigned | ReaderOp::Year4 => {
            f.year = Some(parse_int(matched));
            true
        }
        ReaderOp::Year2 => {
            let v = parse_int(matched);
            f.year = Some(v + if v > 68 { 1900 } else { 2000 });
            true
        }
        ReaderOp::Offset => {
            if matched == "Z" {
                f.offset = Some(0);
                return true;
            }
            let bytes = matched.as_bytes();
            let sign_plus = bytes.first() == Some(&b'+');
            let hh = matched.get(1..3).and_then(|s| s.parse::<i64>().ok()).unwrap_or(0);
            let mm_rest = matched.get(3..).unwrap_or("").trim_start_matches(':');
            let mm = mm_rest.get(0..2).and_then(|s| s.parse::<i64>().ok()).unwrap_or(0);
            let minutes = hh * 60 + mm;
            f.offset = Some(if minutes == 0 {
                0
            } else if sign_plus {
                -minutes
            } else {
                minutes
            });
            true
        }
    }
}

fn remove_first_subslice(haystack: &mut Vec<char>, needle: &[char]) {
    if needle.is_empty() || needle.len() > haystack.len() {
        return;
    }
    if let Some(pos) = (0..=haystack.len() - needle.len()).find(|&i| &haystack[i..i + needle.len()] == needle) {
        haystack.drain(pos..pos + needle.len());
    }
}

fn js_to_number(input: &str) -> f64 {
    let s = input.trim_matches(is_space);
    if s.is_empty() {
        return 0.0;
    }
    if matches!(s, "Infinity" | "+Infinity") {
        return f64::INFINITY;
    }
    if s == "-Infinity" {
        return f64::NEG_INFINITY;
    }
    // Reject Rust-specific `inf`/`nan` spellings that JS `Number()` rejects.
    if s.chars().any(|c| c.is_ascii_alphabetic() && !matches!(c, 'e' | 'E' | 'x' | 'X' | 'b' | 'B' | 'o' | 'O' | 'a'..='f' | 'A'..='F')) {
        return f64::NAN;
    }
    if let Some(hex) = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
        return u128::from_str_radix(hex, 16).map_or(f64::NAN, |v| v as f64);
    }
    if let Some(oct) = s.strip_prefix("0o").or_else(|| s.strip_prefix("0O")) {
        return u128::from_str_radix(oct, 8).map_or(f64::NAN, |v| v as f64);
    }
    if let Some(bin) = s.strip_prefix("0b").or_else(|| s.strip_prefix("0B")) {
        return u128::from_str_radix(bin, 2).map_or(f64::NAN, |v| v as f64);
    }
    s.parse::<f64>().unwrap_or(f64::NAN)
}

fn loose(input: &str, template: &str, now: JsDate) -> Option<JsDate> {
    if template == "x" || template == "X" {
        let factor = if template == "X" { 1000.0 } else { 1.0 };
        return Some(JsDate::from_ms(factor * js_to_number(input)));
    }
    let steps = compile_steps(template)?;
    let mut f = Fields::default();
    let mut rest: Vec<char> = input.chars().collect();
    let mut start = 0usize;

    for step in steps {
        match step {
            Step::Lit(len) => {
                start += len;
            }
            Step::Token(kind, op) => {
                let slice = if start <= rest.len() { &rest[start..] } else { &[] };
                let (m_start, m_end) = find_match(kind, slice)?;
                let matched_chars: Vec<char> = slice[m_start..m_end].to_vec();
                let matched_str: String = matched_chars.iter().collect();
                if !apply_reader(op, &mut f, &matched_str) {
                    return None;
                }
                // dayjs drops the first occurrence of the matched text, wherever it is, and does not move `start`.
                remove_first_subslice(&mut rest, &matched_chars);
            }
        }
    }

    if let (Some(afternoon), Some(hours)) = (f.afternoon, f.hours.as_mut()) {
        if afternoon {
            if *hours < 12 {
                *hours += 12;
            }
        } else if *hours == 12 {
            *hours = 0;
        }
    }

    let year_truthy = f.year.unwrap_or(0) != 0;
    let month_truthy = f.month.unwrap_or(0) != 0;
    let day_truthy = f.day.unwrap_or(0) != 0;

    let day = if f.day_given || day_truthy {
        f.day.unwrap_or(0)
    } else if !year_truthy && !month_truthy {
        now.date()
    } else {
        1
    };
    let year = match f.year {
        Some(y) if y != 0 => y,
        _ => now.full_year(),
    };
    let mut month = 0i64;
    if !(year_truthy && !month_truthy) {
        month = match f.month {
            Some(m) if m > 0 => m - 1,
            _ => now.month(),
        };
    }
    let hours = f.hours.unwrap_or(0);
    let minutes = f.minutes.unwrap_or(0);
    let seconds = f.seconds.unwrap_or(0);
    let ms = f.milliseconds.unwrap_or(0);

    if let Some(offset) = f.offset {
        return Some(JsDate::from_ymd_hms_ms(
            year,
            month,
            day,
            hours,
            minutes,
            seconds,
            ms.saturating_add(offset.saturating_mul(60_000)),
        ));
    }
    if f.week.unwrap_or(0) != 0 {
        return None;
    }
    Some(JsDate::from_ymd_hms_ms(year, month, day, hours, minutes, seconds, ms))
}

// Parses the way `dayjs(input, format, true)` does. `now` supplies the fields the format leaves out.
pub fn parse_strict(input: &str, template: &str, now: JsDate) -> Option<JsDate> {
    let date = loose(input, template, now)?;
    if is_valid(date) && format(date, template).as_deref() == Some(input) {
        Some(date)
    } else {
        None
    }
}

fn parse_time_tail(s: &str) -> Option<(i64, i64, i64, i64, i64)> {
    // Parses `HH:mm(:ss(.sss...)?)?(?:\s*(AM|PM))?(?:\s*(Z|GMT|UTC|[+-]\d\d:?\d\d))?`
    let s = s.trim();
    if s.is_empty() {
        return Some((0, 0, 0, 0, 0));
    }
    let colon = s.find(':')?;
    let hh: i64 = s[..colon].parse().ok()?;
    let rest = &s[colon + 1..];
    if rest.len() < 2 || !rest.as_bytes()[0].is_ascii_digit() || !rest.as_bytes()[1].is_ascii_digit() {
        return None;
    }
    let mm: i64 = rest[..2].parse().ok()?;
    let mut rem = &rest[2..];
    let mut ss = 0i64;
    let mut ms = 0i64;

    if let Some(after_colon) = rem.strip_prefix(':') {
        if after_colon.len() < 2
            || !after_colon.as_bytes()[0].is_ascii_digit()
            || !after_colon.as_bytes()[1].is_ascii_digit()
        {
            return None;
        }
        ss = after_colon[..2].parse().ok()?;
        rem = &after_colon[2..];
        if let Some(after_dot) = rem.strip_prefix('.') {
            let d_len = after_dot.bytes().take_while(|b| b.is_ascii_digit()).count();
            if d_len == 0 {
                return None;
            }
            let mut frac = after_dot[..d_len.min(3)].to_string();
            while frac.len() < 3 {
                frac.push('0');
            }
            ms = frac.parse().ok()?;
            rem = &after_dot[d_len..];
        }
    }

    let mut hours = hh;
    rem = rem.trim_start();
    let lower_rem = rem.to_ascii_lowercase();
    if lower_rem.starts_with("am") || lower_rem.starts_with("pm") {
        let is_pm = lower_rem.starts_with("pm");
        let after_ap = &rem[2..];
        if after_ap.is_empty() || !after_ap.chars().next().unwrap().is_ascii_alphabetic() {
            if !(1..=12).contains(&hours) {
                return None;
            }
            if is_pm && hours < 12 {
                hours += 12;
            } else if !is_pm && hours == 12 {
                hours = 0;
            }
            rem = after_ap.trim_start();
        }
    }

    let mut offset_min = 0i64;
    if !rem.is_empty() {
        if rem.eq_ignore_ascii_case("Z") || rem.eq_ignore_ascii_case("UTC") || rem.eq_ignore_ascii_case("GMT") {
            offset_min = 0;
        } else if rem.starts_with('+') || rem.starts_with('-') {
            let sign = if rem.starts_with('+') { -1i64 } else { 1i64 };
            let digits = rem[1..].replace(':', "");
            if digits.len() != 4 || !digits.bytes().all(|b| b.is_ascii_digit()) {
                return None;
            }
            let oh: i64 = digits[..2].parse().ok()?;
            let om: i64 = digits[2..4].parse().ok()?;
            if oh > 23 || om > 59 {
                return None;
            }
            offset_min = sign * (oh * 60 + om);
        } else {
            return None;
        }
    }

    if mm > 59 || ss > 59 {
        return None;
    }
    if hours > 24 || (hours == 24 && (mm != 0 || ss != 0 || ms != 0)) {
        return None;
    }
    Some((hours, mm, ss, ms, offset_min))
}

fn parse_iso8601(s: &str) -> Option<JsDate> {
    // Matches `^([+-]\d{6}|\d{4})(?:-(\d{2})(?:-(\d{2}))?)?(?:[T ](.+))?$`
    let bytes = s.as_bytes();
    let (year, mut pos) = if (bytes.first() == Some(&b'+') || bytes.first() == Some(&b'-'))
        && bytes.len() >= 7
        && bytes[1..7].iter().all(|b| b.is_ascii_digit())
    {
        let y: i64 = s[..7].parse().ok()?;
        if bytes.first() == Some(&b'-') && y == 0 {
            return None; // `-000000` is rejected by ECMAScript ISO 8601
        }
        (y, 7)
    } else if bytes.len() >= 4 && bytes[..4].iter().all(|b| b.is_ascii_digit()) {
        (s[..4].parse::<i64>().ok()?, 4)
    } else {
        return None;
    };

    let mut month = 1i64;
    let mut day = 1i64;
    if bytes.get(pos) == Some(&b'-') {
        if pos + 3 > bytes.len() || !bytes[pos + 1..pos + 3].iter().all(|b| b.is_ascii_digit()) {
            return None;
        }
        month = s[pos + 1..pos + 3].parse().ok()?;
        pos += 3;
        if bytes.get(pos) == Some(&b'-') {
            if pos + 3 > bytes.len() || !bytes[pos + 1..pos + 3].iter().all(|b| b.is_ascii_digit()) {
                return None;
            }
            day = s[pos + 1..pos + 3].parse().ok()?;
            pos += 3;
        }
    }

    if !(1..=12).contains(&month) || !(1..=civil_days_in_month(year, month)).contains(&day) {
        return None;
    }

    if pos == s.len() {
        return Some(JsDate::from_full_year(year, month - 1, day, 0, 0, 0, 0));
    }

    let sep = bytes[pos];
    if sep != b'T' && sep != b' ' {
        return None;
    }
    let tail = &s[pos + 1..];
    if tail.is_empty() {
        return None;
    }
    let (hh, mm, ss, ms, offset_min) = parse_time_tail(tail)?;
    Some(JsDate::from_full_year(
        year,
        month - 1,
        day,
        hh,
        mm,
        ss,
        ms + offset_min * 60_000,
    ))
}

fn match_month_token(tok: &str) -> Option<i64> {
    if tok.len() < 3 || !tok.bytes().all(|b| b.is_ascii_alphabetic()) {
        return None;
    }
    let lower = tok.to_ascii_lowercase();
    for (idx, m) in MONTHS.iter().enumerate() {
        if m.to_ascii_lowercase().starts_with(&lower) {
            return Some((idx + 1) as i64);
        }
    }
    None
}

// Fallback `new Date(str)` parser for `getStartDate` when `parseStrict` fails.
pub fn parse_js_date(raw: &str) -> JsDate {
    let s = raw.trim_matches(is_space);
    if s.is_empty() {
        return JsDate::NAN;
    }
    if let Some(d) = parse_iso8601(s) {
        return d;
    }

    // Slash or dash numeric formats (`YYYY/M/D`, `M/D/YYYY`, `M-D-YYYY`) with optional time.
    let (date_part, time_part) = match s.find(|c: char| is_space(c)) {
        Some(at) => (&s[..at], s[at..].trim()),
        None => (s, ""),
    };
    if date_part.contains('/') || date_part.contains('-') {
        let sep = if date_part.contains('/') { '/' } else { '-' };
        let parts: Vec<&str> = date_part.split(sep).collect();
        if (parts.len() == 2 || parts.len() == 3)
            && parts.iter().all(|p| !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit()))
        {
            if let Some((hh, mm, ss, ms, offset_min)) = parse_time_tail(time_part) {
                let p0: i64 = parts[0].parse().unwrap_or(-1);
                let p1: i64 = parts[1].parse().unwrap_or(-1);
                if parts[0].len() == 4 {
                    let d = if parts.len() == 3 {
                        parts[2].parse().unwrap_or(-1)
                    } else {
                        1
                    };
                    if (1..=12).contains(&p1) && (1..=civil_days_in_month(p0, p1)).contains(&d) {
                        return JsDate::from_full_year(p0, p1 - 1, d, hh, mm, ss, ms + offset_min * 60_000);
                    }
                } else if parts.len() == 3 && (parts[2].len() == 4 || parts[2].len() == 2) {
                    let mut y: i64 = parts[2].parse().unwrap_or(-1);
                    if parts[2].len() == 2 {
                        y += if y >= 50 { 1900 } else { 2000 };
                    }
                    if (1..=12).contains(&p0) && (1..=civil_days_in_month(y, p0)).contains(&p1) {
                        return JsDate::from_full_year(y, p0 - 1, p1, hh, mm, ss, ms + offset_min * 60_000);
                    }
                }
            }
        }
    }

    // Named-month dates (`Jan 15, 2024`, `15 Jan 2024`, `Mon, 15 Jan 2024 12:00:00 GMT`).
    let mut cleaned = s;
    if let Some(comma) = cleaned.find(',') {
        let prefix = cleaned[..comma].trim();
        if prefix.len() >= 3
            && WEEKDAYS
                .iter()
                .any(|w| w.to_ascii_lowercase().starts_with(&prefix.to_ascii_lowercase()))
        {
            cleaned = cleaned[comma + 1..].trim();
        }
    }
    let tokens: Vec<&str> = cleaned
        .split(|c: char| is_space(c) || c == ',')
        .filter(|t| !t.is_empty())
        .collect();
    if tokens.len() >= 3 {
        let try_mdy = if let Some(m) = match_month_token(tokens[0]) {
            let d = tokens[1].parse::<i64>().ok();
            let y = tokens[2].parse::<i64>().ok();
            d.zip(y).map(|(d, y)| (y, m, d))
        } else if let Some(m) = match_month_token(tokens[1]) {
            let a = tokens[0].parse::<i64>().ok();
            let b = tokens[2].parse::<i64>().ok();
            a.zip(b).map(|(a, b)| if tokens[0].len() == 4 { (a, m, b) } else { (b, m, a) })
        } else {
            None
        };
        if let Some((mut y, m, d)) = try_mdy {
            if (0..=99).contains(&y) && tokens[0].len() != 4 && tokens[2].len() == 2 {
                y += if y >= 50 { 1900 } else { 2000 };
            }
            if (1..=civil_days_in_month(y, m)).contains(&d) {
                let tail = tokens[3..].join(" ");
                if let Some((hh, mm, ss, ms, offset_min)) = parse_time_tail(&tail) {
                    return JsDate::from_full_year(y, m - 1, d, hh, mm, ss, ms + offset_min * 60_000);
                }
            }
        }
    }

    JsDate::NAN
}
