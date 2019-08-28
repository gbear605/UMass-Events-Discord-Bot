use chrono::offset::FixedOffset;
use chrono::prelude::Utc;

use chrono::DateTime;
use chrono::Datelike;
use chrono::Timelike;
use chrono::Weekday;

pub fn get_datetime() -> DateTime<FixedOffset> {
    let current_time_utc = Utc::now();
    let current_time: DateTime<FixedOffset> = DateTime::from_utc(
        current_time_utc.naive_utc(),
        FixedOffset::west(4 * 60 * 60),
        // Four hours west of the date line
        // Four instead of five because 5am/6am is a better default than 6am/7am
    );

    current_time
}

pub fn get_day_of_week() -> Weekday {
    get_datetime().date().weekday()
}

// Runs at 6 AM in summer or 5 AM in winter
pub fn get_time_till_scheduled() -> std::time::Duration {
    let current_time = get_datetime();
    let next_run_date = if current_time.time().hour() < 6
        || (current_time.hour() == 6 && current_time.minute() < 5)
    {
        // We want to do it today (in UTC) if it is still yesterday in Eastern Time
        current_time
    } else {
        current_time + chrono::Duration::days(1)
    }
    .date();

    let next_run = next_run_date.and_hms(6, 5, 0);

    (next_run - current_time).to_std().unwrap()
}
