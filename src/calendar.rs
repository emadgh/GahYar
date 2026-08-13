#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CalendarKind {
    Jalali,
    Gregorian,
    Hijri,
}

impl CalendarKind {
    pub fn key(self) -> &'static str {
        match self {
            Self::Jalali => "jalali",
            Self::Gregorian => "gregorian",
            Self::Hijri => "hijri",
        }
    }

    pub fn from_key(value: &str) -> Self {
        match value {
            "gregorian" => Self::Gregorian,
            "hijri" => Self::Hijri,
            _ => Self::Jalali,
        }
    }

    pub fn title(self) -> &'static str {
        match self {
            Self::Jalali => "شمسی",
            Self::Gregorian => "میلادی",
            Self::Hijri => "قمری",
        }
    }

    pub fn next(self) -> Self {
        match self {
            Self::Jalali => Self::Gregorian,
            Self::Gregorian => Self::Hijri,
            Self::Hijri => Self::Jalali,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Date {
    pub year: i32,
    pub month: u32,
    pub day: u32,
}

impl Date {
    pub const fn new(year: i32, month: u32, day: u32) -> Self {
        Self { year, month, day }
    }
}

pub const JALALI_MONTHS: [&str; 12] = [
    "فروردین",
    "اردیبهشت",
    "خرداد",
    "تیر",
    "مرداد",
    "شهریور",
    "مهر",
    "آبان",
    "آذر",
    "دی",
    "بهمن",
    "اسفند",
];

pub const GREGORIAN_MONTHS_EN: [&str; 12] = [
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

pub const GREGORIAN_MONTHS_FA: [&str; 12] = [
    "ژانویه",
    "فوریه",
    "مارس",
    "آوریل",
    "مه",
    "ژوئن",
    "ژوئیه",
    "اوت",
    "سپتامبر",
    "اکتبر",
    "نوامبر",
    "دسامبر",
];

pub const HIJRI_MONTHS: [&str; 12] = [
    "محرم",
    "صفر",
    "ربیع‌الاول",
    "ربیع‌الثانی",
    "جمادی‌الاول",
    "جمادی‌الثانی",
    "رجب",
    "شعبان",
    "رمضان",
    "شوال",
    "ذی‌القعده",
    "ذی‌الحجه",
];

pub const WEEKDAYS_SHORT: [&str; 7] = ["ش", "ی", "د", "س", "چ", "پ", "ج"];

pub fn month_name(kind: CalendarKind, month: u32) -> &'static str {
    let index = month.saturating_sub(1).min(11) as usize;
    match kind {
        CalendarKind::Jalali => JALALI_MONTHS[index],
        CalendarKind::Gregorian => GREGORIAN_MONTHS_EN[index],
        CalendarKind::Hijri => HIJRI_MONTHS[index],
    }
}

pub fn month_name_fa(kind: CalendarKind, month: u32) -> &'static str {
    let index = month.saturating_sub(1).min(11) as usize;
    match kind {
        CalendarKind::Jalali => JALALI_MONTHS[index],
        CalendarKind::Gregorian => GREGORIAN_MONTHS_FA[index],
        CalendarKind::Hijri => HIJRI_MONTHS[index],
    }
}

pub fn to_gregorian(kind: CalendarKind, date: Date) -> Date {
    match kind {
        CalendarKind::Gregorian => date,
        CalendarKind::Jalali => {
            let (year, month, day) =
                jalali_to_gregorian(date.year, date.month as i32, date.day as i32);
            Date::new(year, month as u32, day as u32)
        }
        CalendarKind::Hijri => jdn_to_gregorian(hijri_to_jdn(date.year, date.month, date.day)),
    }
}

pub fn from_gregorian(kind: CalendarKind, date: Date) -> Date {
    match kind {
        CalendarKind::Gregorian => date,
        CalendarKind::Jalali => {
            let (year, month, day) =
                gregorian_to_jalali(date.year, date.month as i32, date.day as i32);
            Date::new(year, month as u32, day as u32)
        }
        CalendarKind::Hijri => jdn_to_hijri(gregorian_to_jdn(date)),
    }
}

pub fn convert(date: Date, from: CalendarKind, to: CalendarKind) -> Date {
    if from == to {
        date
    } else {
        from_gregorian(to, to_gregorian(from, date))
    }
}

pub fn days_in_month(kind: CalendarKind, year: i32, month: u32) -> u32 {
    match kind {
        CalendarKind::Jalali => jalali_month_length(year, month),
        CalendarKind::Gregorian => gregorian_month_length(year, month),
        CalendarKind::Hijri => hijri_month_length(year, month),
    }
}

pub fn first_weekday_saturday(kind: CalendarKind, year: i32, month: u32) -> i32 {
    let gregorian = to_gregorian(kind, Date::new(year, month, 1));
    weekday_saturday(gregorian)
}

pub fn weekday_saturday(gregorian: Date) -> i32 {
    // Julian day modulo: 0=Monday. Convert to Saturday-first index.
    let weekday_monday_zero = gregorian_to_jdn(gregorian).rem_euclid(7) as i32;
    (weekday_monday_zero + 2) % 7
}

pub fn add_month(_kind: CalendarKind, year: &mut i32, month: &mut u32, delta: i32) {
    let zero_based = (*year as i64) * 12 + (*month as i64 - 1) + delta as i64;
    *year = zero_based.div_euclid(12) as i32;
    *month = (zero_based.rem_euclid(12) + 1) as u32;
}

pub fn add_day(kind: CalendarKind, date: Date, delta: i32) -> Date {
    let mut gregorian = to_gregorian(kind, date);
    let step = if delta < 0 { -1 } else { 1 };
    for _ in 0..delta.unsigned_abs() {
        if step > 0 {
            if gregorian.day
                < days_in_month(CalendarKind::Gregorian, gregorian.year, gregorian.month)
            {
                gregorian.day += 1;
            } else if gregorian.month < 12 {
                gregorian.month += 1;
                gregorian.day = 1;
            } else {
                gregorian.year += 1;
                gregorian.month = 1;
                gregorian.day = 1;
            }
        } else if gregorian.day > 1 {
            gregorian.day -= 1;
        } else if gregorian.month > 1 {
            gregorian.month -= 1;
            gregorian.day = days_in_month(CalendarKind::Gregorian, gregorian.year, gregorian.month);
        } else {
            gregorian.year -= 1;
            gregorian.month = 12;
            gregorian.day = days_in_month(CalendarKind::Gregorian, gregorian.year, gregorian.month);
        }
    }
    from_gregorian(kind, gregorian)
}

pub fn format_heading(kind: CalendarKind, year: i32, month: u32, persian_year: &str) -> String {
    match kind {
        CalendarKind::Gregorian => format!("{} {}", month_name(kind, month), year),
        _ => format!("{} {}", month_name(kind, month), persian_year),
    }
}

pub fn month_range_text(kind: CalendarKind, year: i32, month: u32, target: CalendarKind) -> String {
    let start = convert(Date::new(year, month, 1), kind, target);
    let end = convert(
        Date::new(year, month, days_in_month(kind, year, month)),
        kind,
        target,
    );
    let start_name = month_name(target, start.month);
    let end_name = month_name(target, end.month);
    if start.year == end.year && start.month == end.month {
        format!("{} {}", start_name, start.year)
    } else if start.year == end.year {
        format!("{} – {} {}", start_name, end_name, end.year)
    } else {
        format!("{} {} – {} {}", start_name, start.year, end_name, end.year)
    }
}

pub fn month_range_text_fa(
    kind: CalendarKind,
    year: i32,
    month: u32,
    target: CalendarKind,
    digits: impl Fn(i32) -> String,
) -> String {
    let start = convert(Date::new(year, month, 1), kind, target);
    let end = convert(
        Date::new(year, month, days_in_month(kind, year, month)),
        kind,
        target,
    );
    let start_name = month_name_fa(target, start.month);
    let end_name = month_name_fa(target, end.month);
    if start.year == end.year && start.month == end.month {
        format!("{} {}", start_name, digits(start.year))
    } else if start.year == end.year {
        format!("{} – {} {}", start_name, end_name, digits(end.year))
    } else {
        format!(
            "{} {} – {} {}",
            start_name,
            digits(start.year),
            end_name,
            digits(end.year)
        )
    }
}

fn gregorian_month_length(year: i32, month: u32) -> u32 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if is_gregorian_leap(year) => 29,
        2 => 28,
        _ => 30,
    }
}

fn is_gregorian_leap(year: i32) -> bool {
    (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
}

fn hijri_month_length(year: i32, month: u32) -> u32 {
    if month == 12 {
        if ((11 * year + 14).rem_euclid(30)) < 11 {
            30
        } else {
            29
        }
    } else if month % 2 == 1 {
        30
    } else {
        29
    }
}

fn jalali_month_length(year: i32, month: u32) -> u32 {
    match month {
        1..=6 => 31,
        7..=11 => 30,
        12 => {
            let (gy, gm, gd) = jalali_to_gregorian(year, 12, 30);
            if gregorian_to_jalali(gy, gm, gd) == (year, 12, 30) {
                30
            } else {
                29
            }
        }
        _ => 30,
    }
}

fn gregorian_to_jdn(date: Date) -> i64 {
    let a = (14 - date.month as i64) / 12;
    let y = date.year as i64 + 4800 - a;
    let m = date.month as i64 + 12 * a - 3;
    date.day as i64 + (153 * m + 2) / 5 + 365 * y + y / 4 - y / 100 + y / 400 - 32045
}

fn jdn_to_gregorian(jdn: i64) -> Date {
    let mut l = jdn + 68569;
    let n = 4 * l / 146097;
    l -= (146097 * n + 3) / 4;
    let i = 4000 * (l + 1) / 1461001;
    l = l - 1461 * i / 4 + 31;
    let j = 80 * l / 2447;
    let day = l - 2447 * j / 80;
    l = j / 11;
    let month = j + 2 - 12 * l;
    let year = 100 * (n - 49) + i + l;
    Date::new(year as i32, month as u32, day as u32)
}

fn hijri_to_jdn(year: i32, month: u32, day: u32) -> i64 {
    day as i64
        + (59 * (month as i64 - 1) + 1) / 2
        + (year as i64 - 1) * 354
        + (3 + 11 * year as i64) / 30
        + 1_948_439
        - 1
}

fn jdn_to_hijri(jdn: i64) -> Date {
    let year = ((30 * (jdn - 1_948_439) + 10_646) / 10_631) as i32;
    let first = hijri_to_jdn(year, 1, 1);
    let mut month = (((jdn - (29 + first)) as f64 / 29.5).ceil() as i32 + 1).clamp(1, 12) as u32;
    while month > 1 && jdn < hijri_to_jdn(year, month, 1) {
        month -= 1;
    }
    while month < 12 && jdn >= hijri_to_jdn(year, month + 1, 1) {
        month += 1;
    }
    let day = (jdn - hijri_to_jdn(year, month, 1) + 1) as u32;
    Date::new(year, month, day)
}

pub fn jalali_to_gregorian(jy: i32, jm: i32, jd: i32) -> (i32, i32, i32) {
    let jy = jy + 1595;
    let mut days = -355668
        + 365 * jy
        + (jy / 33) * 8
        + ((jy % 33 + 3) / 4)
        + jd
        + if jm < 7 {
            (jm - 1) * 31
        } else {
            (jm - 7) * 30 + 186
        };
    let mut gy = 400 * (days / 146097);
    days %= 146097;
    if days > 36524 {
        gy += 100 * ((days - 1) / 36524);
        days = (days - 1) % 36524;
        if days >= 365 {
            days += 1;
        }
    }
    gy += 4 * (days / 1461);
    days %= 1461;
    if days > 365 {
        gy += (days - 1) / 365;
        days = (days - 1) % 365;
    }
    let mut gd = days + 1;
    let leap = is_gregorian_leap(gy);
    let month_days = [
        31,
        if leap { 29 } else { 28 },
        31,
        30,
        31,
        30,
        31,
        31,
        30,
        31,
        30,
        31,
    ];
    let mut gm = 1;
    for length in month_days {
        if gd <= length {
            break;
        }
        gd -= length;
        gm += 1;
    }
    (gy, gm, gd)
}

pub fn gregorian_to_jalali(gy: i32, gm: i32, gd: i32) -> (i32, i32, i32) {
    let gdm = [0, 31, 59, 90, 120, 151, 181, 212, 243, 273, 304, 334];
    let gy2 = if gm > 2 { gy + 1 } else { gy };
    let mut days = 355666 + 365 * gy + (gy2 + 3) / 4 - (gy2 + 99) / 100
        + (gy2 + 399) / 400
        + gd
        + gdm[(gm - 1) as usize];
    let mut jy = -1595 + 33 * (days / 12053);
    days %= 12053;
    jy += 4 * (days / 1461);
    days %= 1461;
    if days > 365 {
        jy += (days - 1) / 365;
        days = (days - 1) % 365;
    }
    let (jm, jd) = if days < 186 {
        (1 + days / 31, 1 + days % 31)
    } else {
        (7 + (days - 186) / 30, 1 + (days - 186) % 30)
    };
    (jy, jm, jd)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_date_conversions() {
        let g = Date::new(2026, 8, 9);
        assert_eq!(
            from_gregorian(CalendarKind::Jalali, g),
            Date::new(1405, 5, 18)
        );
        assert_eq!(
            from_gregorian(CalendarKind::Hijri, g),
            Date::new(1448, 2, 25)
        );
        assert_eq!(
            to_gregorian(CalendarKind::Jalali, Date::new(1405, 1, 1)),
            Date::new(2026, 3, 21)
        );
    }

    #[test]
    fn day_navigation_crosses_month_boundaries() {
        assert_eq!(
            add_day(CalendarKind::Gregorian, Date::new(2026, 8, 31), 1),
            Date::new(2026, 9, 1)
        );
        assert_eq!(
            add_day(CalendarKind::Jalali, Date::new(1405, 1, 1), -1),
            Date::new(1404, 12, 29)
        );
    }
}
