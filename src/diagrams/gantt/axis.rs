// Time axis support, following d3-time and d3-time-format (English locale), which Mermaid uses:
// the `axisFormat` specifiers, tick intervals with d3's `every` rule, and automatic ticks.

use super::dates::{JsDate, MONTHS, WEEKDAYS};
use crate::svg::builder::js_number;

const SECOND: f64 = 1_000.0;
const MINUTE: f64 = 60.0 * SECOND;
const HOUR: f64 = 60.0 * MINUTE;
const DAY: f64 = 24.0 * HOUR;
const WEEK: f64 = 7.0 * DAY;
const MONTH: f64 = 30.0 * DAY;
const YEAR: f64 = 365.0 * DAY;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TickUnit {
    Millisecond,
    Second,
    Minute,
    Hour,
    Day,
    Week,
    Month,
    Year,
}

#[derive(Clone, Copy)]
enum Mode {
    Base,
    EveryMs(f64),
    EveryYear(f64),
    EveryWeek { step: f64, epoch: JsDate },
    EveryField(i64),
}

#[derive(Clone, Copy)]
struct Calendar {
    unit: TickUnit,
    week_start: i64,
    cycle: f64,
    ms: f64,
    mode: Mode,
}

fn start_of_day(d: &mut JsDate) {
    d.set_hours(0, 0, 0, 0);
}

fn base_floor(d: &mut JsDate, unit: TickUnit, week_start: i64) {
    match unit {
        TickUnit::Millisecond => {}
        TickUnit::Second => {
            let ms = d.milliseconds() as f64;
            d.set_time(d.time() - ms);
        }
        TickUnit::Minute => {
            let sub = d.milliseconds() as f64 + d.seconds() as f64 * SECOND;
            d.set_time(d.time() - sub);
        }
        TickUnit::Hour => {
            let sub = d.milliseconds() as f64
                + d.seconds() as f64 * SECOND
                + d.minutes() as f64 * MINUTE;
            d.set_time(d.time() - sub);
        }
        TickUnit::Day => {
            start_of_day(d);
        }
        TickUnit::Week => {
            let back = (d.day() + 7 - week_start) % 7;
            d.set_date(d.date() - back);
            start_of_day(d);
        }
        TickUnit::Month => {
            d.set_date(1);
            start_of_day(d);
        }
        TickUnit::Year => {
            d.set_month_day(0, 1);
            start_of_day(d);
        }
    }
}

fn base_offset(d: &mut JsDate, unit: TickUnit, step: f64) {
    match unit {
        TickUnit::Millisecond => d.set_time(d.time() + step),
        TickUnit::Second => d.set_time(d.time() + step * SECOND),
        TickUnit::Minute => d.set_time(d.time() + step * MINUTE),
        TickUnit::Hour => d.set_time(d.time() + step * HOUR),
        TickUnit::Day => d.set_date_f64(d.date() as f64 + step),
        TickUnit::Week => d.set_date_f64(d.date() as f64 + step * 7.0),
        TickUnit::Month => d.set_month_f64(d.month() as f64 + step),
        TickUnit::Year => d.set_full_year_f64(d.full_year() as f64 + step),
    }
}

fn base_field(d: JsDate, unit: TickUnit) -> i64 {
    match unit {
        TickUnit::Second => d.seconds(),
        TickUnit::Minute => d.minutes(),
        TickUnit::Hour => d.hours(),
        TickUnit::Day => d.date() - 1,
        TickUnit::Month => d.month(),
        _ => 0,
    }
}

fn base_meta(unit: TickUnit) -> (f64, f64) {
    match unit {
        TickUnit::Millisecond => (f64::INFINITY, 1.0),
        TickUnit::Second => (60.0, SECOND),
        TickUnit::Minute => (60.0, MINUTE),
        TickUnit::Hour => (24.0, HOUR),
        TickUnit::Day => (31.0, DAY),
        TickUnit::Week => (f64::INFINITY, WEEK),
        TickUnit::Month => (12.0, MONTH),
        TickUnit::Year => (f64::INFINITY, YEAR),
    }
}

// Whole units between two floored dates, discounting a change of UTC offset between them.
fn span(start: JsDate, end: JsDate, unit: f64) -> f64 {
    let tz_diff = (end.timezone_offset() - start.timezone_offset()) as f64 * MINUTE;
    ((end.time() - start.time() - tz_diff) / unit).floor()
}

fn euclid_mod(a: f64, b: f64) -> f64 {
    ((a % b) + b) % b
}

impl Calendar {
    fn floor(&self, d: &mut JsDate) {
        match self.mode {
            Mode::Base => base_floor(d, self.unit, self.week_start),
            Mode::EveryMs(step) => d.set_time((d.time() / step).floor() * step),
            Mode::EveryYear(step) => {
                let yr = (d.full_year() as f64 / step).floor() * step;
                d.set_full_year_f64(yr);
                base_floor(d, TickUnit::Year, self.week_start);
            }
            Mode::EveryWeek { step, epoch } => {
                base_floor(d, TickUnit::Week, self.week_start);
                let rem = euclid_mod(span(epoch, *d, WEEK), step);
                base_offset(d, TickUnit::Week, -rem);
            }
            Mode::EveryField(step) => {
                base_floor(d, self.unit, self.week_start);
                while d.time().is_finite() && base_field(*d, self.unit) % step != 0 {
                    d.set_time(d.time() - 1.0);
                    base_floor(d, self.unit, self.week_start);
                }
            }
        }
    }

    fn offset(&self, d: &mut JsDate, n: f64) {
        match self.mode {
            Mode::Base => base_offset(d, self.unit, n),
            Mode::EveryMs(step) => d.set_time(d.time() + n * step),
            Mode::EveryYear(step) => d.set_full_year_f64(d.full_year() as f64 + n * step),
            Mode::EveryWeek { step, .. } => base_offset(d, TickUnit::Week, n * step),
            Mode::EveryField(step) => {
                let count = n.max(0.0) as usize;
                for _ in 0..count {
                    if !d.time().is_finite() {
                        break;
                    }
                    loop {
                        base_offset(d, self.unit, 1.0);
                        if !d.time().is_finite() || base_field(*d, self.unit) % step == 0 {
                            break;
                        }
                    }
                }
            }
        }
    }
}

// The interval d3's `unit.every(step)` gives.
fn every(unit: TickUnit, step: f64, week_start: i64) -> Calendar {
    let (cycle, ms) = base_meta(unit);
    if step <= 1.0 {
        return Calendar { unit, week_start, cycle, ms, mode: Mode::Base };
    }
    let mode = match unit {
        TickUnit::Millisecond => Mode::EveryMs(step),
        TickUnit::Year => Mode::EveryYear(step),
        TickUnit::Week => {
            let mut epoch = JsDate::from_ms(0.0);
            base_floor(&mut epoch, TickUnit::Week, week_start);
            Mode::EveryWeek { step, epoch }
        }
        _ => Mode::EveryField(step.trunc() as i64),
    };
    Calendar { unit, week_start, cycle, ms, mode }
}

// Ticks from start to stop inclusive, or None when there would be more than `limit`.
fn range(calendar: &Calendar, start: f64, stop: f64, limit: usize) -> Option<Vec<f64>> {
    let mut d = JsDate::from_ms(start - 1.0);
    calendar.floor(&mut d);
    calendar.offset(&mut d, 1.0);
    calendar.floor(&mut d);
    let mut out = Vec::new();
    let mut previous = f64::NEG_INFINITY;
    while d.time() <= stop && d.time() > previous {
        if out.len() >= limit {
            return None;
        }
        previous = d.time();
        out.push(previous);
        calendar.offset(&mut d, 1.0);
        calendar.floor(&mut d);
    }
    Some(out)
}

fn tick_step(start: f64, stop: f64, count: f64) -> f64 {
    let denom = count.max(0.0);
    if denom == 0.0 || stop <= start {
        return 0.0;
    }
    let step = (stop - start) / denom;
    let power = step.log10().floor();
    let base = 10.0_f64.powf(power);
    let error = step / base;
    let factor = if error >= 50.0_f64.sqrt() {
        10.0
    } else if error >= 10.0_f64.sqrt() {
        5.0
    } else if error >= std::f64::consts::SQRT_2 {
        2.0
    } else {
        1.0
    };
    base * factor
}

const AUTO: [(TickUnit, f64, f64); 18] = [
    (TickUnit::Second, 1.0, SECOND),
    (TickUnit::Second, 5.0, 5.0 * SECOND),
    (TickUnit::Second, 15.0, 15.0 * SECOND),
    (TickUnit::Second, 30.0, 30.0 * SECOND),
    (TickUnit::Minute, 1.0, MINUTE),
    (TickUnit::Minute, 5.0, 5.0 * MINUTE),
    (TickUnit::Minute, 15.0, 15.0 * MINUTE),
    (TickUnit::Minute, 30.0, 30.0 * MINUTE),
    (TickUnit::Hour, 1.0, HOUR),
    (TickUnit::Hour, 3.0, 3.0 * HOUR),
    (TickUnit::Hour, 6.0, 6.0 * HOUR),
    (TickUnit::Hour, 12.0, 12.0 * HOUR),
    (TickUnit::Day, 1.0, DAY),
    (TickUnit::Day, 2.0, 2.0 * DAY),
    (TickUnit::Week, 1.0, WEEK),
    (TickUnit::Month, 1.0, MONTH),
    (TickUnit::Month, 3.0, 3.0 * MONTH),
    (TickUnit::Year, 1.0, YEAR),
];

// About `count` ticks at round times, chosen the way d3's time scale chooses them.
pub fn auto_ticks(start: f64, stop: f64, count: usize) -> Vec<f64> {
    let count_f = count as f64;
    let target = (stop - start) / count_f;
    let mut i = 0usize;
    while i < AUTO.len() && AUTO[i].2 <= target {
        i += 1;
    }
    let calendar = if i == AUTO.len() {
        every(TickUnit::Year, tick_step(start / YEAR, stop / YEAR, count_f).max(1.0), 0)
    } else if i == 0 {
        every(TickUnit::Millisecond, tick_step(start, stop, count_f).max(1.0), 0)
    } else {
        let pick = if target / AUTO[i - 1].2 < AUTO[i].2 / target { i - 1 } else { i };
        let (unit, step, _) = AUTO[pick];
        every(unit, step, 0)
    };
    range(&calendar, start, stop, 4 * count + 8).unwrap_or_default()
}

const WEEK_STARTS: [&str; 7] = [
    "sunday",
    "monday",
    "tuesday",
    "wednesday",
    "thursday",
    "friday",
    "saturday",
];

fn parse_interval(interval: &str) -> Option<(f64, TickUnit)> {
    // `/^([1-9]\d*)(millisecond|second|minute|hour|day|week|month)$/`
    let bytes = interval.as_bytes();
    if bytes.is_empty() || !matches!(bytes[0], b'1'..=b'9') {
        return None;
    }
    let mut d_end = 1;
    while d_end < bytes.len() && bytes[d_end].is_ascii_digit() {
        d_end += 1;
    }
    let step: f64 = interval[..d_end].parse().ok()?;
    let unit = match &interval[d_end..] {
        "millisecond" => TickUnit::Millisecond,
        "second" => TickUnit::Second,
        "minute" => TickUnit::Minute,
        "hour" => TickUnit::Hour,
        "day" => TickUnit::Day,
        "week" => TickUnit::Week,
        "month" => TickUnit::Month,
        _ => return None,
    };
    Some((step, unit))
}

// Ticks for a `tickInterval` such as `1week`, or None when invalid or exceeding `limit`.
pub fn interval_ticks(
    interval: &str,
    weekday: &str,
    start: f64,
    stop: f64,
    limit: usize,
) -> Option<Vec<f64>> {
    let (step, unit) = parse_interval(interval)?;
    let week_start = WEEK_STARTS.iter().position(|&w| w == weekday).unwrap_or(0) as i64;
    let calendar = every(unit, step, week_start);
    if (stop - start) / (step.min(calendar.cycle) * calendar.ms) > limit as f64 {
        return None;
    }
    range(&calendar, start, stop, limit)
}

fn pad(value: i64, fill: &str, width: usize) -> String {
    let sign = if value < 0 { "-" } else { "" };
    let digits = value.unsigned_abs().to_string();
    if digits.len() < width {
        format!("{}{}{}", sign, fill.repeat(width - digits.len()), digits)
    } else {
        format!("{}{}", sign, digits)
    }
}

fn floored_unit(date: JsDate, unit: TickUnit, week_start: i64) -> JsDate {
    let mut d = date;
    base_floor(&mut d, unit, week_start);
    d
}

// Weeks since the last `week_start` day before the year began (d3's %U and %W).
fn week_of_year(date: JsDate, week_start: i64) -> i64 {
    let year_start = floored_unit(date, TickUnit::Year, 0);
    let first = floored_unit(JsDate::from_ms(year_start.time() - 1.0), TickUnit::Week, week_start);
    let cur = floored_unit(date, TickUnit::Week, week_start);
    span(first, cur, WEEK) as i64
}

// The Thursday that decides which ISO week and ISO year a date belongs to.
fn iso_thursday(date: JsDate) -> JsDate {
    let day = date.day();
    let seed = if day >= 4 || day == 0 {
        date
    } else {
        JsDate::from_ms(date.time() - 1.0)
    };
    let mut d = floored_unit(seed, TickUnit::Week, 4);
    if !(day >= 4 || day == 0) {
        base_offset(&mut d, TickUnit::Week, 1.0);
        base_floor(&mut d, TickUnit::Week, 4);
    }
    d
}

// Formats a date with a d3 time format specifier such as `%Y-%m-%d`.
pub fn time_format(specifier: &str, date: JsDate) -> String {
    let chars: Vec<char> = specifier.chars().collect();
    let n = chars.len();
    let mut out = String::with_capacity(specifier.len() + 8);
    let mut i = 0usize;

    while i < n {
        if chars[i] != '%' {
            out.push(chars[i]);
            i += 1;
            continue;
        }
        i += 1;
        let mut c = chars.get(i).copied();
        let fill = match c {
            Some('-') => {
                i += 1;
                c = chars.get(i).copied();
                ""
            }
            Some('_') => {
                i += 1;
                c = chars.get(i).copied();
                " "
            }
            Some('0') => {
                i += 1;
                c = chars.get(i).copied();
                "0"
            }
            Some('e') => " ",
            _ => "0",
        };
        if let Some(ch) = c {
            out.push_str(&directive(ch, date, fill));
        }
        i += 1;
    }

    out
}

fn directive(c: char, d: JsDate, p: &str) -> String {
    match c {
        'a' => WEEKDAYS[d.day() as usize][..3].to_string(),
        'A' => WEEKDAYS[d.day() as usize].to_string(),
        'b' => MONTHS[d.month() as usize][..3].to_string(),
        'B' => MONTHS[d.month() as usize].to_string(),
        'c' => time_format("%x, %X", d),
        'd' | 'e' => pad(d.date(), p, 2),
        'f' => format!("{}000", pad(d.milliseconds(), p, 3)),
        'g' => pad(iso_thursday(d).full_year() % 100, p, 2),
        'G' => pad(iso_thursday(d).full_year() % 10000, p, 4),
        'H' => pad(d.hours(), p, 2),
        'I' => {
            let h = d.hours() % 12;
            pad(if h == 0 { 12 } else { h }, p, 2)
        }
        'j' => {
            let doy = 1 + span(floored_unit(d, TickUnit::Year, 0), floored_unit(d, TickUnit::Day, 0), DAY) as i64;
            pad(doy, p, 3)
        }
        'L' => pad(d.milliseconds(), p, 3),
        'm' => pad(d.month() + 1, p, 2),
        'M' => pad(d.minutes(), p, 2),
        'p' => (if d.hours() >= 12 { "PM" } else { "AM" }).to_string(),
        'q' => (1 + d.month() / 3).to_string(),
        'Q' => js_number(d.time()),
        's' => ((d.time() / 1000.0).floor() as i64).to_string(),
        'S' => pad(d.seconds(), p, 2),
        'u' => {
            let wd = d.day();
            (if wd == 0 { 7 } else { wd }).to_string()
        }
        'U' => pad(week_of_year(d, 0), p, 2),
        'V' => {
            let thursday = iso_thursday(d);
            let year_start = floored_unit(thursday, TickUnit::Year, 0);
            let first = floored_unit(year_start, TickUnit::Week, 4);
            let w = span(first, thursday, WEEK) as i64 + if year_start.day() == 4 { 1 } else { 0 };
            pad(w, p, 2)
        }
        'w' => d.day().to_string(),
        'W' => pad(week_of_year(d, 1), p, 2),
        'x' => time_format("%-m/%-d/%Y", d),
        'X' => time_format("%-I:%M:%S %p", d),
        'y' => pad(d.full_year() % 100, p, 2),
        'Y' => pad(d.full_year() % 10000, p, 4),
        'Z' => {
            let z = d.timezone_offset();
            let abs = z.abs();
            let sign = if z > 0 { "-" } else { "+" };
            format!("{}{}{}", sign, pad(abs / 60, "0", 2), pad(abs % 60, "0", 2))
        }
        '%' => "%".to_string(),
        _ => c.to_string(),
    }
}
