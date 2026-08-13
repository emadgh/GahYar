use std::collections::HashSet;
use std::fs;
use std::path::PathBuf;

use serde::Deserialize;

#[derive(Clone, Debug)]
pub struct CalendarEvent {
    pub jalali_year: i32,
    pub jalali_month: u32,
    pub jalali_day: u32,
    pub title: String,
    pub description: String,
    pub category: String,
    pub official_holiday: bool,
}

#[derive(Clone, Debug)]
pub struct EventStore {
    pub source_year: i32,
    pub event_count: usize,
    pub source_label: String,
    events: Vec<CalendarEvent>,
    holiday_dates: HashSet<String>,
}

#[derive(Deserialize)]
struct RawDatabase {
    metadata: RawMetadata,
    official_holiday_dates: Vec<String>,
    events: Vec<RawEvent>,
}

#[derive(Deserialize)]
struct RawMetadata {
    calendar_year: i32,
    #[serde(default)]
    statistics: RawStatistics,
}

#[derive(Default, Deserialize)]
struct RawStatistics {
    #[serde(default)]
    event_count: usize,
}

#[derive(Deserialize)]
struct RawEvent {
    jalali: RawDate,
    title: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    category: String,
    #[serde(default)]
    is_official_holiday: bool,
}

#[derive(Deserialize)]
struct RawDate {
    year: i32,
    month: u32,
    day: u32,
}

impl EventStore {
    pub fn load() -> Self {
        let embedded = include_str!("../assets/iran-calendar-events-1405.json");
        let (json, label) = external_json()
            .and_then(|path| {
                fs::read_to_string(&path)
                    .ok()
                    .map(|content| (content, format!("فایل محلی: {}", path.display())))
            })
            .unwrap_or_else(|| (embedded.to_owned(), "دادهٔ داخلی سال ۱۴۰۵".to_string()));

        match serde_json::from_str::<RawDatabase>(&json) {
            Ok(raw) => {
                let events = raw
                    .events
                    .into_iter()
                    .map(|event| CalendarEvent {
                        jalali_year: event.jalali.year,
                        jalali_month: event.jalali.month,
                        jalali_day: event.jalali.day,
                        title: event.title,
                        description: event.description,
                        category: event.category,
                        official_holiday: event.is_official_holiday,
                    })
                    .collect::<Vec<_>>();
                let count = if raw.metadata.statistics.event_count > 0 {
                    raw.metadata.statistics.event_count
                } else {
                    events.len()
                };
                Self {
                    source_year: raw.metadata.calendar_year,
                    event_count: count,
                    source_label: label,
                    events,
                    holiday_dates: raw.official_holiday_dates.into_iter().collect(),
                }
            }
            Err(error) => Self {
                source_year: 1405,
                event_count: 0,
                source_label: format!("خطا در خواندن JSON: {error}"),
                events: Vec::new(),
                holiday_dates: HashSet::new(),
            },
        }
    }

    pub fn events_for_day(&self, year: i32, month: u32, day: u32) -> Vec<&CalendarEvent> {
        self.events
            .iter()
            .filter(|event| {
                event.jalali_year == year && event.jalali_month == month && event.jalali_day == day
            })
            .collect()
    }

    pub fn events_for_month(&self, year: i32, month: u32) -> Vec<&CalendarEvent> {
        self.events
            .iter()
            .filter(|event| event.jalali_year == year && event.jalali_month == month)
            .collect()
    }

    pub fn is_official_holiday(&self, year: i32, month: u32, day: u32) -> bool {
        self.holiday_dates
            .contains(&format!("{year:04}-{month:02}-{day:02}"))
    }
}

fn external_json() -> Option<PathBuf> {
    let directory = std::env::current_exe().ok()?.parent()?.to_path_buf();
    [
        "iran-calendar-events-1405.json",
        "iran-calendar-events-1405(1).json",
    ]
    .into_iter()
    .map(|name| directory.join(name))
    .find(|path| path.is_file())
}
