extern crate discord;
extern crate reqwest;
extern crate select;

use discord::Discord;
use discord::model::Event;

use select::document::Document;
use select::predicate::Class;

// For file reading
use std::io::Read;
use std::fs::File;

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

fn main() {
    let (discord, mut connection) = login();
    println!("Connected to Discord");
    println!("Connected to servers: {:?}", discord.get_servers());

    loop {
        match connection.recv_event() {
            Ok(Event::MessageCreate(message)) => {
                println!("{} says: {}", message.author.name, message.content);
                if message.content == "!events" {

                    let events = get_events();

                    // Intro
                    let _ =
                        discord.send_message(message.channel_id, "Today's events are:", "", false);

                    let _ = events.iter().map(|event| {
                        discord.send_message(message.channel_id, &event.format(), "", false)
                    });

                } else if message.content == "!quit" {
                    println!("Quitting.");
                    break;
                }
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
