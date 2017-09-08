extern crate discord;
extern crate reqwest;
extern crate select;

use discord::Discord;
use discord::model::Event;

use std::vec;

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
            Some(ref location) => format!("{} at {}:\n{}", self.title, location, self.description),
            None => format!("{}:\n{}", self.title, self.description),   
        }
    }
}

fn get_events() -> Vec<UMassEvent> {


    let mut resp = reqwest::get("http://www.umass.edu/events/")
        .expect("Couldn't get the events page");

    println!("Status: {}", resp.status());

    let mut content = String::new();
    let _ = resp.read_to_string(&mut content);


    let document = Document::from(&*content);

    let mut events: vec::Vec<UMassEvent> = vec![];

    for node in document.find(Class("views-row")) {

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
        events.push(UMassEvent {
            title: title,
            description: description,
            date: date,
            location: location,
        })

    }
    events
}

fn main() {
    let mut token_file = File::open("token").expect("No token file");
    let mut token = String::new();
    let _ = token_file.read_to_string(&mut token);

    let discord = Discord::from_bot_token(token.trim()).expect("Login failed");

    let (mut connection, _) = discord.connect().expect("Connect failed");

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

                    for event in events {
                        let _ =
                            discord.send_message(message.channel_id, &event.format(), "", false);
                    }

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
