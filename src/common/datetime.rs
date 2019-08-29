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
        // Four hours west of the date line, to adjust to Eastern Time in the winter
    );

    current_time
}

pub fn get_day_of_week() -> Weekday {
    get_datetime().date().weekday()
}

static HOUR_TO_RUN_AT: u32 = 5;

// Runs at (HOUR_TO_RUN_AT + 1) in summer or HOUR_TO_RUN_AT in winter
pub fn get_time_till_scheduled() -> std::time::Duration {
    let current_time = get_datetime();

    // We want to do it today if it has yet to happen, or else tomorrow
    let next_run_date = if current_time.time().hour() < HOUR_TO_RUN_AT {
        current_time.date()
    } else {
        current_time.date() + chrono::Duration::days(1)
    };

    let next_run = next_run_date.and_hms(HOUR_TO_RUN_AT, 0, 0);

    (next_run - current_time).to_std().unwrap()
}
