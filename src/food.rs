use crate::events::get_document;

use select::document::Document;
use select::predicate::Attr;
use select::predicate::Class;
use select::predicate::Predicate;

use reqwest;

use std::fmt;

use chrono::offset::FixedOffset;
use chrono::prelude::Utc;
use chrono::DateTime;
use chrono::Datelike;
use chrono::Weekday;
use chrono::Weekday::*;

#[derive(Copy, Clone, Debug)]
pub enum Meal {
    Breakfast,
    Lunch,
    Dinner,
    LateNight,
}

use self::Meal::*;

fn get_day_of_week() -> Weekday {
    let current_time_utc = Utc::now();
    let current_time: DateTime<FixedOffset> = DateTime::from_utc(
        current_time_utc.naive_utc(),
        FixedOffset::west(4 * 60 * 60),
        // Four hours west of the date line
        // Four instead of five because 5am/6am is a better default than 6am/7am
    );

    current_time.date().weekday()
}

impl fmt::Display for Meal {
    fn fmt(&self, formatter: &mut fmt::Formatter) -> Result<(), fmt::Error> {
        formatter.write_str(match get_day_of_week() {
            Mon | Tue | Wed | Thu | Fri => match self {
                Breakfast => "Breakfast",
                Lunch => "Lunch",
                Dinner => "Dinner",
                LateNight => "Late Night",
            },
            Sat | Sun => match self {
                Breakfast => "Breakfast",
                Lunch => "Brunch",
                Dinner => "Dinner",
                LateNight => "Late Night",
            },
        })
    }
}

#[derive(Copy, Clone, Debug)]
pub enum DiningCommon {
    Berk,
    Hamp,
    Frank,
    Worcester,
}

use self::DiningCommon::*;

fn get_meal_code(meal: Meal) -> String {
    match meal {
        Breakfast => "breakfast_menu",
        Lunch => "lunch_menu",
        Dinner => "dinner_menu",
        LateNight => "latenight_menu",
    }.to_string()
}

fn get_dining_common_code(dining_common: DiningCommon) -> String {
    match dining_common {
        Worcester => "worcester",
        Frank => "franklin",
        Hamp => "hampshire",
        Berk => "berkshire",
    }.to_string()
}

fn get_menu_document(dining_common: DiningCommon) -> Result<Document, reqwest::Error> {
    let dining_common_id = get_dining_common_code(dining_common);

    let url: &str = &format!(
        "http://umassdining.com/locations-menus/{dining_common}/menu",
        dining_common = dining_common_id
    );

    println!("{}", url);

    get_document(url)
}

pub fn get_on_menu(dining_common: DiningCommon, meal: Meal, item: &str) -> Vec<String> {
    let nodes: Vec<String> = get_menu_document(dining_common)
        .expect("Couldn't get the menu page")
        .find(Attr("id", &get_meal_code(meal)[..]).descendant(Attr("id", "content_text")))
        .nth(0)
        .expect("Couldn't find the menu items on page")
        .find(Class("lightbox-nutrition"))
        .map(|node| node.text())
        .collect();

    let filtered: Vec<String> = nodes
        .into_iter()
        .map(|text| text.to_lowercase())
        .filter(|text| text.contains(item.to_lowercase().as_str()))
        .collect();

    println!("{}", filtered.join(" "));

    filtered
}

fn which_meals(dc: DiningCommon) -> Vec<Meal> {
    match get_day_of_week() {
        Mon | Tue | Wed | Thu => match dc {
            Berk => vec![Lunch, Dinner, LateNight],
            Hamp | Frank => vec![Breakfast, Lunch, Dinner],
            Worcester => vec![Breakfast, Lunch, Dinner, LateNight],
        },
        Fri => match dc {
            Berk => vec![Lunch, Dinner, LateNight],
            Hamp | Frank | Worcester => vec![Breakfast, Lunch, Dinner],
        },
        Sat => match dc {
            Berk => vec![Lunch, Dinner, LateNight],
            Hamp | Frank | Worcester => vec![Lunch, Dinner],
        },
        Sun => match dc {
            Berk | Worcester => vec![Lunch, Dinner, LateNight],
            Hamp | Frank => vec![Lunch, Dinner],
        },
    }
}

pub fn check_for(food: &str) -> String {
    let mut places: Vec<String> = vec![];

    for dining_common in &[Berk, Hamp, Frank, Worcester] {
        let meals = which_meals(*dining_common);
        for meal in meals {
            let food_on_menu = get_on_menu(*dining_common, meal, food);
            if !food_on_menu.is_empty() {
                places.push(
                    format!("{:?} {}: {}", dining_common, meal, food_on_menu.join(", "))
                        .to_string(),
                );
            }
        }
    }

    let to_return = match places.len() {
        0 => format!("{} not found", food).to_string(),
        _ => format!("{}: \n{}", food, places.join("\n")).to_string(),
    };

    println!("To return: {}", to_return);

    to_return
}
