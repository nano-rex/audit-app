//! Time: the clock, calendar arithmetic, and the local time zone.
//!
//! The local zone comes from TZ or /etc/localtime, read as a TZif file (RFC 8536), including its
//! POSIX rule for times after the last listed transition. Calendar conversions use the
//! days-from-civil algorithms of Howard Hinnant's date library.

use std::sync::OnceLock;
use std::time::{SystemTime, UNIX_EPOCH};

pub fn now_secs() -> f64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|elapsed| elapsed.as_secs_f64()).unwrap_or(0.0)
}

/// Milliseconds since the epoch, as int(time.time() * 1000).
pub fn now_ms() -> i64 {
    (now_secs() * 1000.0) as i64
}

pub fn days_from_civil(year: i64, month: u32, day: u32) -> i64 {
    let year = if month <= 2 { year - 1 } else { year };
    let era = if year >= 0 { year } else { year - 399 } / 400;
    let year_of_era = year - era * 400;
    let month = month as i64;
    let day_of_year = (153 * (if month > 2 { month - 3 } else { month + 9 }) + 2) / 5 + day as i64 - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146_097 + day_of_era - 719_468
}

pub fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let days = days + 719_468;
    let era = if days >= 0 { days } else { days - 146_096 } / 146_097;
    let day_of_era = days - era * 146_097;
    let year_of_era = (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_index = (5 * day_of_year + 2) / 153;
    let day = (day_of_year - (153 * month_index + 2) / 5 + 1) as u32;
    let month = if month_index < 10 { month_index + 3 } else { month_index - 9 } as u32;
    let year = year_of_era + era * 400 + if month <= 2 { 1 } else { 0 };
    (year, month, day)
}

pub fn is_leap(year: i64) -> bool {
    year % 4 == 0 && (year % 100 != 0 || year % 400 == 0)
}

pub fn days_in_month(year: i64, month: u32) -> u32 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        _ if is_leap(year) => 29,
        _ => 28,
    }
}

/// A calendar date.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Date {
    pub year: i64,
    pub month: u32,
    pub day: u32,
}

impl Date {
    pub fn new(year: i64, month: u32, day: u32) -> Option<Date> {
        if (1..=9999).contains(&year) && (1..=12).contains(&month) && day >= 1 && day <= days_in_month(year, month) {
            Some(Date { year, month, day })
        } else {
            None
        }
    }

    pub fn from_days(days: i64) -> Date {
        let (year, month, day) = civil_from_days(days);
        Date { year, month, day }
    }

    pub fn days(&self) -> i64 {
        days_from_civil(self.year, self.month, self.day)
    }

    pub fn add_days(&self, count: i64) -> Date {
        Date::from_days(self.days() + count)
    }

    /// date.fromisoformat for the YYYY-MM-DD form.
    pub fn parse_iso(text: &str) -> Option<Date> {
        let bytes = text.as_bytes();
        if bytes.len() != 10 || bytes[4] != b'-' || bytes[7] != b'-' {
            return None;
        }
        let digits = |range: std::ops::Range<usize>| -> Option<i64> {
            let part = &text[range];
            if part.bytes().all(|b| b.is_ascii_digit()) {
                part.parse().ok()
            } else {
                None
            }
        };
        Date::new(digits(0..4)?, digits(5..7)? as u32, digits(8..10)? as u32)
    }

    /// datetime.strptime(text, "%Y-%m-%d"): four-digit year, one or two digits for month and day.
    pub fn parse_ymd(text: &str) -> Option<Date> {
        let mut parts = text.split('-');
        let year = parts.next()?;
        let month = parts.next()?;
        let day = parts.next()?;
        if parts.next().is_some() || year.len() != 4 || !(1..=2).contains(&month.len()) || !(1..=2).contains(&day.len()) {
            return None;
        }
        if ![year, month, day].iter().all(|part| part.bytes().all(|b| b.is_ascii_digit())) {
            return None;
        }
        Date::new(year.parse().ok()?, month.parse().ok()?, day.parse().ok()?)
    }

    pub fn iso(&self) -> String {
        format!("{:04}-{:02}-{:02}", self.year, self.month, self.day)
    }
}

/// A wall-clock time in some zone.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DateTime {
    pub date: Date,
    pub hour: u32,
    pub minute: u32,
    pub second: u32,
    pub micros: u32,
    /// Seconds east of UTC.
    pub offset: i32,
}

impl DateTime {
    pub fn from_unix(seconds: f64, offset: i32) -> DateTime {
        let whole = seconds.floor();
        let micros = ((seconds - whole) * 1_000_000.0).round().min(999_999.0) as u32;
        let local = whole as i64 + offset as i64;
        let days = local.div_euclid(86_400);
        let rest = local.rem_euclid(86_400);
        DateTime {
            date: Date::from_days(days),
            hour: (rest / 3600) as u32,
            minute: (rest % 3600 / 60) as u32,
            second: (rest % 60) as u32,
            micros,
            offset,
        }
    }

    /// "%Y-%m-%d"
    pub fn date_text(&self) -> String {
        self.date.iso()
    }

    /// "%H:%M"
    pub fn hm(&self) -> String {
        format!("{:02}:{:02}", self.hour, self.minute)
    }

    /// "%Y-%m-%d %H:%M:%S"
    pub fn date_time_text(&self) -> String {
        format!("{} {:02}:{:02}:{:02}", self.date.iso(), self.hour, self.minute, self.second)
    }

    /// "%Y-%m-%d %H:%M"
    pub fn minutes_text(&self) -> String {
        format!("{} {}", self.date.iso(), self.hm())
    }

    /// datetime.isoformat() of a naive local time.
    pub fn iso_naive(&self) -> String {
        let base = format!("{}T{:02}:{:02}:{:02}", self.date.iso(), self.hour, self.minute, self.second);
        if self.micros > 0 {
            format!("{base}.{:06}", self.micros)
        } else {
            base
        }
    }

    /// datetime.isoformat() of an aware time: with its offset, +00:00 for UTC.
    pub fn iso_aware(&self) -> String {
        let sign = if self.offset < 0 { '-' } else { '+' };
        let offset = self.offset.abs();
        format!("{}{sign}{:02}:{:02}", self.iso_naive(), offset / 3600, offset % 3600 / 60)
    }
}

pub fn utc_now() -> DateTime {
    DateTime::from_unix(now_secs(), 0)
}

pub fn local_now() -> DateTime {
    local_at(now_secs())
}

pub fn local_at(seconds: f64) -> DateTime {
    DateTime::from_unix(seconds, zone().offset_at(seconds.floor() as i64))
}

pub fn local_from_ms(milliseconds: i64) -> DateTime {
    local_at(milliseconds as f64 / 1000.0)
}

/// The local date today, as datetime.now().strftime("%Y-%m-%d").
pub fn today() -> Date {
    local_now().date
}

/// Seconds since the epoch of local midnight starting `date` (datetime(...).timestamp()).
pub fn local_midnight(date: Date) -> i64 {
    let naive = date.days() * 86_400;
    // The offset in force at that wall time: try the offset at the naive instant, then correct.
    let first = zone().offset_at(naive) as i64;
    let second = zone().offset_at(naive - first) as i64;
    naive - second
}

/// The rule for a part of the year in a POSIX TZ string ("M3.2.0/2").
#[derive(Debug, Clone, Copy)]
struct Rule {
    month: u32,
    week: u32,
    weekday: u32,
    time: i64,
}

impl Rule {
    /// The UTC instant the rule takes effect in `year`, given the offset in force before it.
    fn instant(&self, year: i64, offset_before: i32) -> i64 {
        let first = days_from_civil(year, self.month, 1);
        let first_weekday = (first + 4).rem_euclid(7) as u32; // 1970-01-01 was a Thursday
        let mut day = 1 + (self.weekday + 7 - first_weekday) % 7 + (self.week.saturating_sub(1)) * 7;
        let last = days_in_month(year, self.month);
        while day > last {
            day -= 7;
        }
        (first + day as i64 - 1) * 86_400 + self.time - offset_before as i64
    }
}

#[derive(Debug, Clone, Default)]
struct Posix {
    standard: i32,
    daylight: Option<(i32, Rule, Rule)>,
}

impl Posix {
    fn offset_at(&self, seconds: i64) -> i32 {
        let Some((daylight, start, end)) = self.daylight else {
            return self.standard;
        };
        let year = Date::from_days((seconds + self.standard as i64).div_euclid(86_400)).year;
        let begins = start.instant(year, self.standard);
        let ends = end.instant(year, daylight);
        let inside = if begins < ends { seconds >= begins && seconds < ends } else { seconds >= begins || seconds < ends };
        if inside {
            daylight
        } else {
            self.standard
        }
    }

    fn parse(text: &str) -> Option<Posix> {
        let mut rest = text.trim();
        let name = |rest: &mut &str| -> Option<()> {
            if let Some(stripped) = rest.strip_prefix('<') {
                let end = stripped.find('>')?;
                *rest = &stripped[end + 1..];
            } else {
                let end = rest.find(|c: char| !c.is_ascii_alphabetic()).unwrap_or(rest.len());
                if end < 3 {
                    return None;
                }
                *rest = &rest[end..];
            }
            Some(())
        };
        let clock = |rest: &mut &str| -> Option<i64> {
            let mut sign = 1;
            if let Some(stripped) = rest.strip_prefix('-') {
                sign = -1;
                *rest = stripped;
            } else if let Some(stripped) = rest.strip_prefix('+') {
                *rest = stripped;
            }
            let end = rest.find(|c: char| !(c.is_ascii_digit() || c == ':')).unwrap_or(rest.len());
            let parts: Vec<i64> = rest[..end].split(':').map(|part| part.parse().ok()).collect::<Option<Vec<_>>>()?;
            *rest = &rest[end..];
            let seconds = parts.first()? * 3600 + parts.get(1).unwrap_or(&0) * 60 + parts.get(2).unwrap_or(&0);
            Some(sign * seconds)
        };
        name(&mut rest)?;
        let standard = -clock(&mut rest)? as i32;
        if rest.is_empty() {
            return Some(Posix { standard, daylight: None });
        }
        name(&mut rest)?;
        let daylight_offset = if rest.starts_with(',') { standard + 3600 } else { -clock(&mut rest)? as i32 };
        let rule = |rest: &mut &str| -> Option<Rule> {
            *rest = rest.strip_prefix(",M")?;
            let end = rest.find(['/', ',']).unwrap_or(rest.len());
            let fields: Vec<u32> = rest[..end].split('.').map(|part| part.parse().ok()).collect::<Option<Vec<_>>>()?;
            *rest = &rest[end..];
            let mut time = 7200;
            if let Some(stripped) = rest.strip_prefix('/') {
                *rest = stripped;
                time = clock(rest)?;
            }
            Some(Rule { month: *fields.first()?, week: *fields.get(1)?, weekday: *fields.get(2)?, time })
        };
        let start = rule(&mut rest)?;
        let end = rule(&mut rest)?;
        Some(Posix { standard, daylight: Some((daylight_offset, start, end)) })
    }
}

#[derive(Debug, Clone, Default)]
pub struct Zone {
    transitions: Vec<i64>,
    kinds: Vec<u8>,
    offsets: Vec<i32>,
    footer: Option<Posix>,
}

impl Zone {
    pub fn offset_at(&self, seconds: i64) -> i32 {
        if self.transitions.is_empty() || seconds >= *self.transitions.last().unwrap() {
            if let Some(footer) = &self.footer {
                return footer.offset_at(seconds);
            }
        }
        match self.transitions.partition_point(|&at| at <= seconds) {
            0 => self.offsets.first().copied().unwrap_or(0),
            index => self.offsets.get(self.kinds[index - 1] as usize).copied().unwrap_or(0),
        }
    }

    fn parse(data: &[u8]) -> Option<Zone> {
        if data.len() < 44 || &data[..4] != b"TZif" {
            return None;
        }
        let read32 = |at: usize| -> Option<u32> { data.get(at..at + 4).map(|b| u32::from_be_bytes([b[0], b[1], b[2], b[3]])) };
        let counts = |at: usize| -> Option<[usize; 6]> {
            let mut out = [0usize; 6];
            for (index, slot) in out.iter_mut().enumerate() {
                *slot = read32(at + 20 + index * 4)? as usize;
            }
            Some(out)
        };
        let [isut, isstd, leap, time, types, chars] = counts(0)?;
        let v1_size = time * 5 + types * 6 + chars + leap * 8 + isstd + isut;
        let (start, wide) = if data[4] >= b'2' { (44 + v1_size, true) } else { (0, false) };
        let header = if wide { counts(start)? } else { [isut, isstd, leap, time, types, chars] };
        let [isut, isstd, leap, time, types, chars] = header;
        let mut at = start + 44;
        let width = if wide { 8 } else { 4 };
        let mut transitions = Vec::with_capacity(time);
        for index in 0..time {
            let bytes = data.get(at + index * width..at + index * width + width)?;
            transitions.push(if wide { i64::from_be_bytes(bytes.try_into().ok()?) } else { i32::from_be_bytes(bytes.try_into().ok()?) as i64 });
        }
        at += time * width;
        let kinds = data.get(at..at + time)?.to_vec();
        at += time;
        let mut offsets = Vec::with_capacity(types);
        for index in 0..types {
            let bytes = data.get(at + index * 6..at + index * 6 + 4)?;
            offsets.push(i32::from_be_bytes(bytes.try_into().ok()?));
        }
        at += types * 6 + chars + leap * (width + 4) + isstd + isut;
        let footer = if wide {
            let rest = data.get(at..)?;
            let text = std::str::from_utf8(rest).ok()?.trim_matches('\n');
            Posix::parse(text.lines().next().unwrap_or(""))
        } else {
            None
        };
        Some(Zone { transitions, kinds, offsets, footer })
    }
}

fn load_zone() -> Zone {
    let configured = std::env::var("TZ").unwrap_or_default();
    let name = configured.trim_start_matches(':');
    if !name.is_empty() {
        let candidates = if name.starts_with('/') {
            vec![std::path::PathBuf::from(name)]
        } else {
            ["/usr/share/zoneinfo", "/usr/lib/zoneinfo", "/usr/share/lib/zoneinfo"].iter().map(|dir| std::path::Path::new(dir).join(name)).collect()
        };
        for path in candidates {
            if let Some(zone) = std::fs::read(path).ok().and_then(|data| Zone::parse(&data)) {
                return zone;
            }
        }
        if let Some(posix) = Posix::parse(name) {
            return Zone { footer: Some(posix), ..Zone::default() };
        }
        return Zone::default();
    }
    std::fs::read("/etc/localtime").ok().and_then(|data| Zone::parse(&data)).unwrap_or_default()
}

static ZONE: OnceLock<Zone> = OnceLock::new();

pub fn zone() -> &'static Zone {
    ZONE.get_or_init(load_zone)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn calendar() {
        assert_eq!(days_from_civil(1970, 1, 1), 0);
        assert_eq!(civil_from_days(20_000), (2024, 10, 4));
        assert_eq!(Date::parse_iso("2026-02-29"), None);
        assert_eq!(Date::parse_iso("2028-02-29").unwrap().add_days(1).iso(), "2028-03-01");
        assert_eq!(Date::parse_ymd("2026-1-5").unwrap().iso(), "2026-01-05");
        assert_eq!(Date::parse_iso("2026-1-5"), None);
        let moment = DateTime::from_unix(1_790_000_000.25, 8 * 3600);
        assert_eq!(moment.iso_aware(), "2026-09-21T22:13:20.250000+08:00");
    }

    #[test]
    fn posix_rules() {
        let zone = Posix::parse("CET-1CEST,M3.5.0,M10.5.0/3").unwrap();
        // 2026-07-01 12:00 UTC is summer time; 2026-12-01 is winter time.
        assert_eq!(zone.offset_at(days_from_civil(2026, 7, 1) * 86_400 + 43_200), 7200);
        assert_eq!(zone.offset_at(days_from_civil(2026, 12, 1) * 86_400), 3600);
        assert_eq!(Posix::parse("<+08>-8").unwrap().offset_at(0), 28_800);
    }
}
