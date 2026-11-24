//! The history behind the players-online charts.
//!
//! Steam only answers "how many players now", so the history is recorded here: every time the Home page gets a fresh
//! estimate (see [`crate::steam_api::players_estimate`]) one sample is stored, at most one every five minutes and kept
//! for 31 days. The charts, the peaks and the "usual" busiest times are worked out from those samples
//! ([`snapshot`]), so they fill in while SKradle is used on this computer; nothing is downloaded and nothing is sent.
//!
//! `player-history.json` in the config folder, written so a person can read it:
//!
//! ```json
//! {
//!   "version": 1,
//!   "all_time_peak": [1793950000, 412],
//!   "samples": [
//!     [1793949000, 338],
//!     [1793949300, 341]
//!   ]
//! }
//! ```
//!
//! Times are Unix seconds (UTC). The calendar views (hour of day, weekday, day buckets) use the time zone offset the
//! caller passes, so the app shows the player's own clock.

use std::io;
use std::path::Path;

use serde::{Deserialize, Serialize};

pub const FILE: &str = "player-history.json";
const VERSION: u32 = 1;
const DAY: i64 = 86_400;
const HOUR: i64 = 3_600;
/// Samples are kept this long.
const KEEP: i64 = 31 * DAY;
/// At most one sample this often.
const MIN_GAP: i64 = 300;
/// A hard cap, so a broken clock cannot grow the file without bound.
const MAX_SAMPLES: usize = 12_000;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct History {
    /// The highest estimate ever recorded here: `(time, players)`.
    pub all_time_peak: Option<(i64, u32)>,
    /// `(time, players)`, oldest first.
    pub samples: Vec<(i64, u32)>,
}

#[derive(Deserialize)]
struct Stored {
    #[serde(default)]
    version: u32,
    #[serde(default)]
    all_time_peak: Option<(i64, u32)>,
    #[serde(default)]
    samples: Vec<(i64, u32)>,
}

impl History {
    /// Reads the file; anything missing or damaged is an empty history (a history only costs a few charts).
    pub fn load(path: &Path) -> Self {
        let Ok(bytes) = crate::bounded::read_regular(path, 4 * 1024 * 1024) else {
            return Self::default();
        };
        let Ok(stored) = serde_json::from_slice::<Stored>(&bytes) else {
            return Self::default();
        };
        if stored.version != VERSION {
            return Self::default();
        }
        let mut samples = stored.samples;
        samples.sort_unstable();
        Self {
            all_time_peak: stored.all_time_peak,
            samples,
        }
    }

    /// One sample per line inside an indented file.
    pub fn to_json(&self) -> String {
        let mut out = format!("{{\n  \"version\": {VERSION},\n");
        match self.all_time_peak {
            Some((t, n)) => out.push_str(&format!("  \"all_time_peak\": [{t}, {n}],\n")),
            None => out.push_str("  \"all_time_peak\": null,\n"),
        }
        out.push_str("  \"samples\": [");
        for (i, (t, n)) in self.samples.iter().enumerate() {
            out.push_str(if i == 0 { "\n    " } else { ",\n    " });
            out.push_str(&format!("[{t}, {n}]"));
        }
        out.push_str(if self.samples.is_empty() {
            "]\n}\n"
        } else {
            "\n  ]\n}\n"
        });
        out
    }

    pub fn save(&self, path: &Path) -> io::Result<()> {
        if let Some(dir) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
            std::fs::create_dir_all(dir)?;
        }
        crate::jar::write_atomic(path, self.to_json().as_bytes())
    }

    /// Adds a sample unless the last one is too recent; old samples fall away. Returns whether anything changed.
    pub fn record(&mut self, now: i64, players: u32) -> bool {
        if self.all_time_peak.is_none_or(|(_, peak)| players > peak) {
            self.all_time_peak = Some((now, players));
        }
        if self
            .samples
            .last()
            .is_some_and(|(t, _)| (now - t).abs() < MIN_GAP)
        {
            return false;
        }
        self.samples.push((now, players));
        self.samples.retain(|(t, _)| now - t <= KEEP);
        if self.samples.len() > MAX_SAMPLES {
            let extra = self.samples.len() - MAX_SAMPLES;
            self.samples.drain(..extra);
        }
        true
    }

    /// A believable month, for screenshots and the self-test: a daily rhythm with a weekend lift. Never written.
    pub fn fixture(now: i64) -> Self {
        let mut history = Self::default();
        let start = now - 30 * DAY;
        let mut t = start - start.rem_euclid(1800);
        while t <= now {
            let hour = (t.rem_euclid(DAY)) as f64 / HOUR as f64;
            let day = (t.div_euclid(DAY) + 3).rem_euclid(7);
            let rhythm = ((hour - 15.0) / 24.0 * std::f64::consts::TAU).cos() * 0.5 + 0.5;
            let weekend = if day >= 5 { 1.18 } else { 1.0 };
            let wobble = ((t / 1800) % 7) as f64 * 3.0;
            let n = ((170.0 + 190.0 * rhythm) * weekend + wobble) as u32;
            history.samples.push((t, n));
            if history.all_time_peak.is_none_or(|(_, p)| n > p) {
                history.all_time_peak = Some((t, n));
            }
            t += 1800;
        }
        history
    }
}

/// Records one sample in the file at `path` (read, add, write). Failing only costs a sample.
pub fn record_sample(path: &Path, now: i64, players: u32) {
    let mut history = History::load(path);
    let changed = history.record(now, players);
    // The peak can change without a new sample (a sample came too soon); it is still worth writing.
    if changed || history.all_time_peak == Some((now, players)) {
        let _ = history.save(path);
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Peak {
    pub t: i64,
    pub n: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct WeekPeak {
    /// 1 = Monday .. 7 = Sunday.
    pub weekday: u32,
    pub hour: u32,
}

/// What the dialog draws.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Snapshot {
    pub current: Option<u32>,
    /// 1 up, -1 down, 0 flat or unknown, over the last hour.
    pub trend: i32,
    pub day_peak: Option<u32>,
    pub all_time_peak: Option<Peak>,
    /// The hour of the day (0 to 23, local) that is busiest on average; needs two days of samples.
    pub usual_day_peak_hour: Option<u32>,
    /// The busiest weekday and hour on average; needs a week of samples.
    pub usual_week_peak: Option<WeekPeak>,
    /// `[time, players]`: the last 24 hours as recorded.
    pub day: Vec<[i64; 2]>,
    /// The last 7 days, the busiest sample of each hour.
    pub week: Vec<[i64; 2]>,
    /// The last 30 days, the busiest sample of each local day.
    pub month: Vec<[i64; 2]>,
    /// How many days the recorded samples span.
    pub span_days: f64,
}

/// Works out the charts and figures at `now`; `tz_offset_min` is minutes to subtract from UTC to get local time
/// (JavaScript's `Date.getTimezoneOffset()`).
pub fn snapshot(history: &History, now: i64, tz_offset_min: i32) -> Snapshot {
    let offset = i64::from(tz_offset_min) * 60;
    let local = |t: i64| t - offset;
    let samples = &history.samples;
    let current = samples.last().map(|(_, n)| *n);
    let span_days = match (samples.first(), samples.last()) {
        (Some((a, _)), Some((b, _))) => (b - a) as f64 / DAY as f64,
        _ => 0.0,
    };

    let day: Vec<[i64; 2]> = samples
        .iter()
        .filter(|(t, _)| now - t <= DAY)
        .map(|(t, n)| [*t, i64::from(*n)])
        .collect();
    let day_peak = day.iter().map(|p| p[1] as u32).max();

    let bucket = |size: i64, keep: i64, shift: i64| -> Vec<[i64; 2]> {
        let mut out: Vec<[i64; 2]> = Vec::new();
        for (t, n) in samples.iter().filter(|(t, _)| now - t <= keep) {
            let key = (t + shift).div_euclid(size);
            let at = key * size - shift;
            match out.last_mut() {
                Some(last) if last[0] == at => last[1] = last[1].max(i64::from(*n)),
                _ => out.push([at, i64::from(*n)]),
            }
        }
        out
    };
    let week = bucket(HOUR, 7 * DAY, 0);
    let month = bucket(DAY, 30 * DAY, -offset);

    // Over the last hour: the latest sample against the average of the samples 30 to 90 minutes ago.
    let earlier: Vec<f64> = samples
        .iter()
        .filter(|(t, _)| (30 * 60..=90 * 60).contains(&(now - t)))
        .map(|(_, n)| f64::from(*n))
        .collect();
    let trend = match (current, earlier.len()) {
        (Some(c), len) if len > 0 => {
            let before = earlier.iter().sum::<f64>() / len as f64;
            if f64::from(c) > before * 1.02 {
                1
            } else if f64::from(c) < before * 0.98 {
                -1
            } else {
                0
            }
        }
        _ => 0,
    };

    // The busiest hour of the day, and of the week, on average.
    let mut by_hour = [(0u64, 0u64); 24];
    let mut by_week = [(0u64, 0u64); 7 * 24];
    for (t, n) in samples {
        let l = local(*t);
        let hour = (l.rem_euclid(DAY) / HOUR) as usize;
        let weekday = (l.div_euclid(DAY) + 3).rem_euclid(7) as usize;
        by_hour[hour].0 += u64::from(*n);
        by_hour[hour].1 += 1;
        by_week[weekday * 24 + hour].0 += u64::from(*n);
        by_week[weekday * 24 + hour].1 += 1;
    }
    let best = |slots: &[(u64, u64)]| -> Option<usize> {
        slots
            .iter()
            .enumerate()
            .filter(|(_, (_, count))| *count > 0)
            .max_by(|(_, (sa, ca)), (_, (sb, cb))| (sa * cb).cmp(&(sb * ca)))
            .map(|(i, _)| i)
    };
    let usual_day_peak_hour = (span_days >= 2.0)
        .then(|| best(&by_hour))
        .flatten()
        .map(|i| i as u32);
    let usual_week_peak = (span_days >= 7.0)
        .then(|| best(&by_week))
        .flatten()
        .map(|i| WeekPeak {
            weekday: (i / 24) as u32 + 1,
            hour: (i % 24) as u32,
        });

    Snapshot {
        current,
        trend,
        day_peak,
        all_time_peak: history.all_time_peak.map(|(t, n)| Peak { t, n }),
        usual_day_peak_hour,
        usual_week_peak,
        day,
        week,
        month,
        span_days,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A Monday, 2026-11-23 00:00 UTC.
    const MONDAY: i64 = 1_795_392_000;

    #[test]
    fn samples_are_spaced_pruned_and_the_peak_is_kept() {
        let mut h = History::default();
        assert!(h.record(MONDAY, 100));
        assert!(!h.record(MONDAY + 60, 90), "too soon");
        assert!(h.record(MONDAY + 600, 300));
        assert!(h.record(MONDAY + 1200, 120));
        assert_eq!(h.all_time_peak, Some((MONDAY + 600, 300)));
        assert_eq!(h.samples.len(), 3);
        // A month and more later the first ones fall away; the peak stays.
        assert!(h.record(MONDAY + 40 * DAY, 50));
        assert_eq!(h.samples, vec![(MONDAY + 40 * DAY, 50)]);
        assert_eq!(h.all_time_peak, Some((MONDAY + 600, 300)));
    }

    #[test]
    fn the_file_round_trips_and_is_one_sample_per_line() {
        let mut h = History::default();
        h.record(MONDAY, 100);
        h.record(MONDAY + 600, 300);
        let json = h.to_json();
        assert!(json.contains("\"samples\": [\n    [1795392000, 100],\n    [1795392600, 300]\n  ]"));
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(FILE);
        h.save(&path).unwrap();
        assert_eq!(History::load(&path), h);
        assert_eq!(History::default().to_json().matches('\n').count(), 5);
    }

    #[test]
    fn a_missing_or_damaged_file_is_an_empty_history() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(FILE);
        assert_eq!(History::load(&path), History::default());
        std::fs::write(&path, "{ not json").unwrap();
        assert_eq!(History::load(&path), History::default());
        std::fs::write(&path, r#"{"version": 9, "samples": [[1, 2]]}"#).unwrap();
        assert_eq!(History::load(&path), History::default(), "a newer format");
        record_sample(&path, MONDAY, 120);
        assert_eq!(History::load(&path).samples, vec![(MONDAY, 120)]);
    }

    #[test]
    fn the_snapshot_has_series_peaks_and_usual_times() {
        let now = MONDAY + 30 * DAY;
        let h = History::fixture(now);
        let s = snapshot(&h, now, 0);
        assert_eq!(s.current, h.samples.last().map(|(_, n)| *n));
        assert!(s.day.len() >= 47 && s.day.len() <= 49, "{}", s.day.len());
        assert!(s.week.len() >= 7 * 24 - 1 && s.week.len() <= 7 * 24 + 1);
        assert!(s.month.len() >= 29 && s.month.len() <= 31);
        assert!(s.day_peak.unwrap() <= s.all_time_peak.as_ref().unwrap().n);
        assert!(s.span_days > 29.0);
        // The fixture is busiest around 15:00 UTC, and on weekends.
        assert!((13..=17).contains(&s.usual_day_peak_hour.unwrap()));
        let week = s.usual_week_peak.unwrap();
        assert!(week.weekday >= 6, "{week:?}");
        // The same moment, seen from UTC+3: the busiest hour moves with the clock.
        let shifted = snapshot(&h, now, -180);
        assert_eq!(
            shifted.usual_day_peak_hour.unwrap(),
            (s.usual_day_peak_hour.unwrap() + 3) % 24
        );
    }

    #[test]
    fn little_data_shows_charts_but_no_usual_times() {
        let mut h = History::default();
        for i in 0..20 {
            h.record(MONDAY + i * 600, 100 + i as u32);
        }
        let s = snapshot(&h, MONDAY + 20 * 600, 0);
        assert_eq!(s.current, Some(119));
        assert_eq!(s.usual_day_peak_hour, None);
        assert_eq!(s.usual_week_peak, None);
        assert_eq!(snapshot(&History::default(), MONDAY, 0).current, None);
    }

    #[test]
    fn the_trend_follows_the_last_hour() {
        let mut h = History::default();
        for i in 0..12 {
            h.record(MONDAY + i * 600, 100);
        }
        let now = MONDAY + 11 * 600 + 300;
        assert_eq!(snapshot(&h, now, 0).trend, 0);
        h.record(now, 150);
        assert_eq!(snapshot(&h, now, 0).trend, 1);
        let mut down = History::default();
        for i in 0..12 {
            down.record(MONDAY + i * 600, 100);
        }
        down.record(now, 60);
        assert_eq!(snapshot(&down, now, 0).trend, -1);
    }
}
