extern crate reqwest;
extern crate select;
extern crate serenity;

use select::document::Document;
use select::predicate::Attr;
use select::predicate::Class;
use select::predicate::Predicate;

use serenity::client::Client;
use serenity::model::*;
use serenity::prelude::*;

// For file reading
use std::fs::File;
use std::fs::OpenOptions;
use std::io::Read;
use std::io::Write;

// For multithreading
use std::ops::DerefMut;
use std::sync::{Arc, Mutex};
use std::thread;

// Allow openssl crosscompiling to work
extern crate openssl_probe;

extern crate time;

#[derive(Debug)]
struct UMassEvent {
    title: String,
    description: String,
    date: String,
    location: Option<String>,
}

impl UMassEvent {
    fn format(&self) -> String {
        match self.location {
            // "Event_Name at Event_location: Long Description"
            Some(ref location) => format!("{} at {}:\n{}", self.title, location, self.description),
            // "Event_Name: Long Description"
            None => format!("{}:\n{}", self.title, self.description),
        }
    }
}

fn get_document(url: &str) -> Result<select::document::Document, reqwest::Error> {
    reqwest::get(url).map(|mut response| {
        // Extract the data from the http request
        let mut content = String::new();
        let _ = response.read_to_string(&mut content);
        Document::from(&*content)
    })
}

fn get_events() -> Vec<UMassEvent> {
    let document =
        get_document("http://www.umass.edu/events/").expect("Couldn't get the events page");

    // Parse the data into a list of events
    document
        .find(Class("views-row"))
        .map(|node| {
            // This is really janky and relies on UMass not changing the event page html...

            let title = node
                .find(Class("views-field-title"))
                .next()
                .unwrap()
                .children()
                .nth(1)
                .unwrap()
                .first_child()
                .unwrap()
                .first_child()
                .unwrap()
                .text();
            let description = node
                .find(Class("views-field-field-short-desc"))
                .next()
                .unwrap()
                .children()
                .nth(1)
                .unwrap()
                .first_child()
                .unwrap()
                .text();
            let date = node.find(Class("event-date")).next().unwrap().text();
            let location = node
                .find(Class("event-location"))
                .next()
                .unwrap()
                .children()
                .nth(1)
                .map(|node| node.first_child().unwrap().text());

            // Return the event, which will then be collected into the events vector
            UMassEvent {
                title: title,
                description: description,
                date: date,
                location: location,
            }
        })
        .collect()
}

#[derive(Copy, Clone, Debug)]
enum Meal {
    Breakfast,
    Lunch,
    Dinner,
    LateNight,
}

impl std::fmt::Display for Meal {
    fn fmt(&self, formatter: &mut std::fmt::Formatter) -> std::result::Result<(), std::fmt::Error> {
        formatter.write_str(match self {
            Meal::Breakfast => "Breakfast",
            Meal::Lunch => "Lunch",
            Meal::Dinner => "Dinner",
            Meal::LateNight => "Late Night",
        })
    }
}

#[derive(Copy, Clone, Debug)]
enum DiningCommon {
    Berk,
    Hamp,
    Frank,
    Worcester,
}

fn get_meal_code(meal: Meal, dining_common: DiningCommon) -> String {
    match dining_common {
        DiningCommon::Worcester => match meal {
            Meal::Breakfast => "breakfast_menu",
            Meal::Lunch => "lunch_menu",
            Meal::Dinner => "dinner_menu",
            Meal::LateNight => "latenight_menu",
        },
        DiningCommon::Frank => match meal {
            Meal::Breakfast => "breakfast_menu",
            Meal::Lunch => "lunch_menu",
            Meal::Dinner => "dinner_menu",
            Meal::LateNight => panic!("Frank doesn't have late night"),
        },
        DiningCommon::Hamp => match meal {
            Meal::Breakfast => "breakfast_menu",
            Meal::Lunch => "lunch_menu",
            Meal::Dinner => "dinner_menu",
            Meal::LateNight => panic!("Hamp doesn't have late night"),
        },
        DiningCommon::Berk => match meal {
            Meal::Breakfast => panic!("Berk doesn't have breakfast"),
            Meal::Lunch => "lunch_menu",
            Meal::Dinner => "dinner_menu",
            Meal::LateNight => "latenight_menu",
        },
    }.to_string()
}

fn get_dining_common_code(dining_common: DiningCommon) -> String {
    match dining_common {
        DiningCommon::Worcester => "worcester",
        DiningCommon::Frank => "franklin",
        DiningCommon::Hamp => "hampshire",
        DiningCommon::Berk => "berkshire",
    }.to_string()
}

fn get_menu_document(
    dining_common: DiningCommon,
) -> Result<select::document::Document, reqwest::Error> {
    let dining_common_id = get_dining_common_code(dining_common);

    let url: &str = &format!(
        "http://umassdining.com/locations-menus/{dining_common}/menu",
        dining_common = dining_common_id
    );

    println!("{}", url);

    get_document(url)
}

fn get_on_menu(dining_common: DiningCommon, meal: Meal, item: &str) -> Vec<String> {
    let nodes: Vec<String> = get_menu_document(dining_common)
        .expect("Couldn't get the menu page")
        .find(
            Attr("id", &get_meal_code(meal, dining_common)[..])
                .descendant(Attr("id", "content_text")),
        )
        .nth(0)
        .expect("Couldn't find the menu items on page")
        .find(Class("lightbox-nutrition"))
        .map(|node| node.text())
        .collect();

    nodes
        .into_iter()
        .map(|text| text.to_lowercase())
        .filter(|text| text.contains(item.to_lowercase().as_str()))
        .collect()

    //println!("{}", filtered.join(" "));
}

fn is_on_menu(dining_common: DiningCommon, meal: Meal, item: &str) -> bool {
    let filtered = get_on_menu(dining_common, meal, item);

    println!("{}", filtered.join(" "));

    !filtered.is_empty()
}

// Get the token file from memory
fn load_token() -> String {
    let mut token = String::new();
    let _ = File::open("token")
        .expect("No token file")
        .read_to_string(&mut token);
    token
}

// Login to Discord and connect
fn login(listeners: Arc<Mutex<Vec<(ChannelId, String)>>>) -> Client<Handler> {
    Client::new(
        load_token().trim(),
        Handler {
            listeners: listeners,
        },
    )
}

fn check_for(food: &str) -> String {
    let mut places: Vec<String> = vec![];

    for dining_common in &[
        DiningCommon::Berk,
        DiningCommon::Hamp,
        DiningCommon::Frank,
        DiningCommon::Worcester,
    ] {
        let meals = match *dining_common {
            DiningCommon::Berk => vec![Meal::Lunch, Meal::Dinner, Meal::LateNight],
            DiningCommon::Hamp | DiningCommon::Frank => {
                vec![Meal::Breakfast, Meal::Lunch, Meal::Dinner]
            }
            DiningCommon::Worcester => {
                vec![Meal::Breakfast, Meal::Lunch, Meal::Dinner, Meal::LateNight]
            }
        };
        for meal in meals {
            if is_on_menu(*dining_common, meal, food) {
                places.push(
                    format!(
                        "{:?} {}: {}",
                        dining_common,
                        meal,
                        get_on_menu(*dining_common, meal, food).join(", ")
                    ).to_string(),
                );
            }
        }
    }

    match places.len() {
        0 => format!("{} not found", food).to_string(),
        _ => format!("{}: \n{}", food, places.join("\n")).to_string(),
    }
}

fn get_guilds() -> Vec<String> {
    let cache = serenity::CACHE.read().unwrap();
    let guilds = cache.all_guilds();
    guilds
        .into_iter()
        .map(|guild| guild.get().unwrap().name)
        .collect()
}

struct Handler {
    listeners: Arc<Mutex<Vec<(serenity::model::ChannelId, String)>>>,
}

impl EventHandler for Handler {
    fn on_message(&self, _ctx: Context, message: Message) {
        let listeners = Arc::clone(&self.listeners);

        if !message.content.starts_with('!') {
            // It's not a command, so we don't care about it
            return;
        }

        if message.author.bot {
            // We don't want it to respond to other bots or itself!
            return;
        }

        println!(
            "{}: {} says: {}",
            message.author.name, message.author.id, message.content
        );

        let is_owner: bool = message.author.id == 90927967651262464;

        if message.content == "!events" {
            let events = get_events();

            // Intro
            let _ = message.channel_id.say("Today's events are:".to_string());

            let _ = events
                .iter()
                .map(|event| message.channel_id.say(event.format().to_string()));
        } else if message.content.starts_with("!menu ") {
            let item: &str = &message.content[6..];

            let _ = message
                .channel_id
                .say(format!("Checking for {}\n", item).to_string());
            let _ = message.channel_id.say(check_for(item));
        } else if message.content.starts_with("!register ") {
            let item: String = message.content[10..].to_string();
            listeners
                .lock()
                .unwrap()
                .deref_mut()
                .push((message.channel_id, item.clone()));
            save_listeners(listeners.lock().unwrap().deref_mut());
            let _ = message
                .channel_id
                .say(format!("Will check for {}", item).to_string());
        } else if message.content == "!help" {
            let _ = message.channel_id.say("UMass Bot help:");
            let _ = message.channel_id.say(
                "```!menu [food name]     | tells you where that food is being served \
                 today```",
            );
            let _ = message.channel_id.say(
                "```!register [food name] | schedules it to tell you each day where that \
                 food is being served that day```",
            );
        } else if message.content == "!run" {
            let _ = message.channel_id.say("Checking for preregistered foods");
            check_for_foods(&listeners);
        } else if message.content.starts_with("!guilds") && is_owner {
            let _ = message
                .channel_id
                .say(format!("Guilds: {}", get_guilds().join(", ")));
        } else if message.content.starts_with("!quit") && is_owner {
            let _ = message.channel_id.say("UMass Bot Quitting");
            std::process::exit(0);
        }
    }
}

fn read_listeners() -> Vec<(serenity::model::ChannelId, String)> {
    let mut listeners_string: String = String::new();
    let _ = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .open("listeners.txt")
        .expect("No listeners file")
        .read_to_string(&mut listeners_string);

    let mut listeners: Vec<(serenity::model::ChannelId, String)> = vec![];

    for line in listeners_string.split('\n') {
        if line == "" {
            continue;
        }
        let sections: Vec<&str> = line.split(' ').collect();
        let id = serenity::model::ChannelId(
            sections[0]
                .parse::<u64>()
                .expect("Couldn't parse channel id"),
        );
        let food: String = sections[1..].join(" ").to_string();
        listeners.push((id, food));
    }

    listeners
}

fn save_listeners(pairs: &[(serenity::model::ChannelId, String)]) {
    let mut listeners_string: String = String::new();
    pairs.into_iter().for_each(|x| {
        let (ref id, ref food) = *x;
        listeners_string = format!("{}\n{} {}", listeners_string, id, food);
    });

    let _ = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(true)
        .open("listeners.txt")
        .unwrap()
        .write_all(listeners_string.as_bytes());
}

// Runs at 6 AM
fn get_time_till_scheduled() -> std::time::Duration {
    // The server the bot is deployed on is in UTC, so we have to adjust by 5 hours
    let current_time = time::now();
    let mut next_midnight: time::Tm =
        if current_time.tm_hour < 11 || (current_time.tm_hour == 11 && current_time.tm_min < 5) {
            // We want to do it today (in UTC) if it is still yesterday in Eastern Time
            current_time.to_local()
        } else {
            (current_time + time::Duration::days(1)).to_local()
        };
    next_midnight.tm_sec = 0;
    next_midnight.tm_min = 5;
    next_midnight.tm_hour = 11;

    (next_midnight - current_time).to_std().unwrap()
}

fn check_for_foods(listeners: &Arc<Mutex<Vec<(ChannelId, String)>>>) {
    listeners
        .lock()
        .unwrap()
        .to_vec()
        .into_iter()
        .for_each(|(channel, food)| {
            println!("Checking on {:?} for {}", channel, food);
            let _ = channel.say(check_for(&food));
        });
}

fn main() {
    // Allow openssl crosscompiling to work
    openssl_probe::init_ssl_cert_env_vars();

    let listeners: Arc<Mutex<Vec<(ChannelId, String)>>> = Arc::new(Mutex::new(read_listeners()));
    let mut client = login(Arc::clone(&listeners));
    println!("Connected to Discord");
    println!("Connected to servers: {}", get_guilds().join(", "));

    // Listeners loop
    let listeners_clone = Arc::clone(&listeners);
    thread::spawn(move || {
        let listeners = listeners_clone;
        loop {
            println!("Seconds till scheduled: {:?}", get_time_till_scheduled());
            thread::sleep(get_time_till_scheduled());
            println!("Checking for foods now!");
            check_for_foods(&listeners);
        }
    });

    let _ = client.start();
}
