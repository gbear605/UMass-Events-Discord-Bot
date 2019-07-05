use serde::de::Deserializer;
use serde::de::{self, Visitor};
use serde::Deserialize;

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::Mutex;

use std::fs::File;
use std::io::Read;

use std::fmt;

use chrono::NaiveTime;

#[derive(Debug, Clone, Copy, Deserialize, PartialEq)]
enum Day {
    Monday,
    Tuesday,
    Wednesday,
    Thursday,
    Friday,
    Saturday,
    Sunday,
}

#[derive(Debug, Deserialize, PartialEq, Clone)]
pub struct Section {
    #[serde(deserialize_with = "deserialize_naive_time")]
    start_time: NaiveTime,
    #[serde(deserialize_with = "deserialize_naive_time")]
    end_time: NaiveTime,
    days: Vec<Day>,
    room: String,
    number: String,
}

fn deserialize_naive_time<'de, D>(deserializer: D) -> Result<NaiveTime, D::Error>
where
    D: Deserializer<'de>,
{
    deserializer.deserialize_identifier(NaiveTimeVisitor)
}

struct NaiveTimeVisitor;

impl<'de> Visitor<'de> for NaiveTimeVisitor {
    type Value = NaiveTime;

    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str("a string in hh:mmAM or hh:mmPM format")
    }

    fn visit_string<E>(self, value: String) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        match NaiveTime::parse_from_str(&value, "%I:%M%p") {
            Ok(ok) => Ok(ok),
            Err(e) => Err(E::custom(e)),
        }
    }

    fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        match NaiveTime::parse_from_str(value, "%I:%M%p") {
            Ok(ok) => Ok(ok),
            Err(e) => Err(E::custom(e)),
        }
    }
}

// TODO: we want the start and end times to be a type that is easy to compare
// Idea for it: make a new struct Time that contains a String for the time
// and then implement Eq for it.

#[derive(Debug, Deserialize, PartialEq, Clone)]
struct Class {
    name: String,
    sections: Vec<Section>,
}

// Get the json file from memory
fn load_class_data() -> Vec<Class> {
    let mut spire_json = String::new();
    let _ = File::open("spire.json")
        .expect("No spire json file")
        .read_to_string(&mut spire_json);

    serde_json::from_str(spire_json.trim()).unwrap()
}

pub type RoomStore = HashMap<String, Vec<Section>>;

pub fn load_sections_map() -> RoomStore {
    let classes = load_class_data();

    let mut rooms_with_sections: HashMap<String, Vec<Section>> = HashMap::new();

    for class in classes {
        for section in class.sections {
            if rooms_with_sections.contains_key(&section.room) {
                let room: &mut Vec<Section> = rooms_with_sections.get_mut(&section.room).unwrap();
                room.push(section);
            } else {
                rooms_with_sections.insert(section.room.clone(), vec![section]);
            }
        }
    }

    rooms_with_sections
}
