use std::io::Read;

use reqwest;
use select::document::Document;
use select::predicate::Class;

// Allow openssl crosscompiling to work
extern crate openssl_probe;

#[derive(Debug)]
pub struct UMassEvent {
    title: String,
    description: String,
    date: String,
    location: Option<String>,
}

impl UMassEvent {
    pub fn format(&self) -> String {
        match self.location {
            // "Event_Name at Event_location: Long Description"
            Some(ref location) => format!("{} at {}:\n{}", self.title, location, self.description),
            // "Event_Name: Long Description"
            None => format!("{}:\n{}", self.title, self.description),
        }
    }
}

pub fn get_document(url: &str) -> Result<String, reqwest::Error> {
    reqwest::get(url).map(|mut response| {
        // Extract the data from the http request
        let mut content = String::new();
        let _ = response.read_to_string(&mut content);
        content
    })
}

pub fn get_events() -> Vec<UMassEvent> {
    let document =
        get_document("http://www.umass.edu/events/").expect("Couldn't get the events page");
    let document = Document::from(&*document);

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
                title,
                description,
                date,
                location,
            }
        })
        .collect()
}
