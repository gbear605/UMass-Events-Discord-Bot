extern crate discord;
extern crate reqwest;
extern crate select;

use discord::Discord;
use discord::model::Event;
use discord::model::Message;

use select::document::Document;
use select::predicate::Class;

// For file reading
use std::io::Read;
use std::fs::File;

// For multithreading
use std::thread;
use std::sync::mpsc::channel;
use std::sync::mpsc::Sender;

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

fn get_document() -> Result<select::document::Document, reqwest::Error> {
    reqwest::get("http://www.umass.edu/events/").map(|mut response| {
        // Extract the data from the http request
        let mut content = String::new();
        let _ = response.read_to_string(&mut content);
        Document::from(&*content)
    })
}

fn get_events() -> Vec<UMassEvent> {
    let document = get_document().expect("Couldn't get the events page");

    // Parse the data into a list of events
    let events = document.find(Class("views-row"))
        .map(|node| {

            // This is really janky and relies on UMass not changing the event page html...

            let title = node.find(Class("views-field-title"))
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
            let description = node.find(Class("views-field-field-short-desc"))
                .next()
                .unwrap()
                .children()
                .nth(1)
                .unwrap()
                .first_child()
                .unwrap()
                .text();
            let date = node.find(Class("event-date")).next().unwrap().text();
            let location = node.find(Class("event-location"))
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
        .collect();

    events
}

#[derive(Copy)]
#[derive(Clone)]
#[derive(Debug)]
enum Meal {
    Breakfast,
    Lunch,
    Dinner,
    LateNight
}

#[derive(Copy)]
#[derive(Clone)]
#[derive(Debug)]
enum DiningCommon {
    Berk,
    Hamp,
    Frank,
    Worcester,
}

fn get_meal_code(meal: Meal, dining_common: DiningCommon) -> String {
    match dining_common {
        DiningCommon::Worcester => {
            match meal {
                Meal::Breakfast => "0700001", 
                Meal::Lunch => "1130001",
                Meal::Dinner => "1630001",
                Meal::LateNight => "2100001"
            }
        },
        DiningCommon::Frank => {
            match meal {
                Meal::Breakfast => "0700002", 
                Meal::Lunch => "1130002",
                Meal::Dinner => "1630002",
                Meal::LateNight => panic!("Frank doesn't have late night")
            }
        },
        DiningCommon::Hamp => {
            match meal {
                Meal::Breakfast => "0700003", 
                Meal::Lunch => "1130003",
                Meal::Dinner => "1630003",
                Meal::LateNight => panic!("Hamp doesn't have late night")
            }
        },
        DiningCommon::Berk => {
            match meal {
                Meal::Breakfast => panic!("Berk doesn't have breakfast"), 
                Meal::Lunch => "1100004",
                Meal::Dinner => "1630004",
                Meal::LateNight => "2100004"
            }
        }
    }

    .to_string()
    
}

fn get_dining_common_code(dining_common: DiningCommon) -> String {
    match dining_common {
        DiningCommon::Worcester => "0",
        DiningCommon::Frank => "1",
        DiningCommon::Hamp => "2",
        DiningCommon::Berk => "3"
    }.to_string()
}

fn get_menu_document(dining_common: DiningCommon, meal: Meal) -> Result<select::document::Document, reqwest::Error> {
    let dining_common_id = get_dining_common_code(dining_common.clone());

    let time = time::now();

    let year = time.tm_year + 1900;
    let month = time.tm_mon + 1;
    let day = time.tm_mday;
    let meal = get_meal_code(meal, dining_common);

    let url: &str = &format!("https://go.umass.edu/dining/event?feed=dining-halls&id=id_{id}\
                                                                                 &calendar=dining-halls_id_{id}_event_calendar\
                                                                                 &startdate={day}-{month}-{year}\
                                                                                 &event={year}{month}{day}T{meal}%7C{year}{month}{day}T000000\
                                                                                 &calendarMode=day", 
                         id=dining_common_id,
                         year=year,
                         month=month,
                         day=day,
                         meal=meal);

    reqwest::get(url).map(|mut response| {
        // Extract the data from the http request
        let mut content = String::new();
        let _ = response.read_to_string(&mut content);
        Document::from(&*content)
    })
    
}

fn is_on_menu(dining_common: DiningCommon, meal: Meal, item: &str) -> bool {
    let nodes: Vec<String> = get_menu_document(dining_common, meal).expect("Couldn't get the menu page").find(Class("kgo_web_content")).map(|node| node.text()).collect();

    let text: String = nodes.join(" ");


    return text.to_lowercase().as_str().contains(item.to_lowercase().as_str());
}

// Get the token file from memory
fn load_token() -> String {
    let mut token = String::new();
    let _ = File::open("token").expect("No token file").read_to_string(&mut token);
    token
}

// Login to Discord and connect
fn login() -> (discord::Discord, discord::Connection) {
    let discord = Discord::from_bot_token(load_token().trim()).expect("Login failed");
    let connection = discord.connect().expect("Connect failed").0;
    (discord, connection)
}

fn message_handler(tx: Sender<(discord::model::ChannelId,String)>, message: Message) {
    thread::spawn(move|| {
        println!("{} says: {}", message.author.name, message.content);
        if message.content == "!events" {

            let events = get_events();

            // Intro
            let _ = tx.send((message.channel_id, "Today's events are:".to_string()));

            let _ = events.iter().map(|event| {
                tx.send((message.channel_id, event.format().to_string()))
            });

        } else if message.content.starts_with("!menu ") {

            let item: String = message.content[6..].to_string();

            let _ = tx.send((message.channel_id, format!("Checking for {}", item).to_string()));

            let mut places: Vec<String> = vec![];

            for dining_common in vec![DiningCommon::Berk, DiningCommon::Hamp, DiningCommon::Frank, DiningCommon::Worcester] {
                let meals = match dining_common.clone() {
                    DiningCommon::Berk => vec![Meal::Lunch, Meal::Dinner, Meal::LateNight],
                    DiningCommon::Hamp => vec![Meal::Breakfast, Meal::Lunch, Meal::Dinner],
                    DiningCommon::Frank => vec![Meal::Breakfast, Meal::Lunch, Meal::Dinner], 
                    DiningCommon::Worcester => vec![Meal::Breakfast, Meal::Lunch, Meal::Dinner, Meal::LateNight]
                };
                for meal in meals {
                    if is_on_menu(dining_common, meal, &item) {
                        places.push(format!("{:?} {:?}", dining_common.clone(), meal.clone()).to_string());
                    }
                }
            }

            let response: String = match places.len() {
                0 => format!("{} not found", item).to_string(),
                _ => format!("{}: {}", item, places.join(", ")).to_string()
            };

            let _ = tx.send((message.channel_id, response));


        } else if message.content == "!quit" {
            println!("Quitting.");
            panic!("Quitting");
        }
    });
}

fn main() {
    let (discord, mut connection) = login();
    println!("Connected to Discord");
    println!("Connected to servers: {:?}", discord.get_servers());


    let (tx, rx) = channel::<(discord::model::ChannelId,String)>();

    thread::spawn(move|| {
        loop {
            let (id, message) = rx.recv().unwrap();
            let _ = discord.send_message(id, &message, "", false);
        }
    });
    loop {
        match connection.recv_event() {
            Ok(Event::MessageCreate(message)) => {
                message_handler(tx.clone(), message);
            }
            Ok(_) => {}
            Err(discord::Error::Closed(code, body)) => {
                println!("Gateway closed on us with code {:?}: {}", code, body);
                break;
            }
            Err(err) => println!("Receive error: {:?}", err),
        }
    }
}
