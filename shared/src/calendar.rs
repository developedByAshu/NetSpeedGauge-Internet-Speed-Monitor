//! Calendar arithmetic on day numbers, where day 0 is 1 January 1970.

pub const MONTH_NAMES: [&str; 12] = [
    "January", "February", "March", "April", "May", "June", "July", "August", "September", "October",
    "November", "December",
];

/// Returns the day number of a calendar date.
pub fn day_from_date(year: i32, month: u32, day: u32) -> i32 {
    let shifted_year = if month <= 2 { year - 1 } else { year };
    let era = shifted_year.div_euclid(400);
    let year_of_era = shifted_year.rem_euclid(400);
    let shifted_month = (month as i32 + 9) % 12;
    let day_of_year = (153 * shifted_month + 2) / 5 + day as i32 - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146_097 + day_of_era - 719_468
}

/// Returns the (year, month, day) of a day number.
pub fn date_from_day(day_number: i32) -> (i32, u32, u32) {
    let shifted = day_number + 719_468;
    let era = shifted.div_euclid(146_097);
    let day_of_era = shifted.rem_euclid(146_097);
    let year_of_era =
        (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let shifted_month = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * shifted_month + 2) / 5 + 1;
    let month = if shifted_month < 10 { shifted_month + 3 } else { shifted_month - 9 };
    let year = year_of_era + era * 400 + i32::from(month <= 2);
    (year, month as u32, day as u32)
}

/// Returns the Monday that starts the week holding this day.
pub fn week_start(day_number: i32) -> i32 {
    // Day 0 was a Thursday, so adding 3 makes Monday land on a multiple of 7.
    day_number - (day_number + 3).rem_euclid(7)
}

/// Returns the first day of the month holding this day.
pub fn month_start(day_number: i32) -> i32 {
    let (year, month, _) = date_from_day(day_number);
    day_from_date(year, month, 1)
}

/// Returns the first day of the year holding this day.
pub fn year_start(day_number: i32) -> i32 {
    day_from_date(date_from_day(day_number).0, 1, 1)
}

/// Returns a short date such as "3 Nov".
pub fn short_date(day_number: i32) -> String {
    let (_, month, day) = date_from_day(day_number);
    format!("{day} {}", &MONTH_NAMES[month as usize - 1][..3])
}
