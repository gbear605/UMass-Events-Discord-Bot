extern crate reqwest;
extern crate select;
#[macro_use]
extern crate serenity;

use serenity::client::Client;
use serenity::model::channel::Message;
use serenity::model::id::ChannelId;
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

mod events;
mod food;

// Get the token file from memory
fn load_token() -> String {
    let mut token = String::new();
    let _ = File::open("token")
        .expect("No token file")
        .read_to_string(&mut token);
    token
}

// Login to Discord and connect
fn login(listeners: Arc<Mutex<Vec<(ChannelId, String)>>>) -> Client {
    Client::new(load_token().trim(), Handler { listeners }).expect("Error creating client")
}

fn get_guilds() -> Vec<String> {
    let cache = serenity::CACHE.read();
    let guilds = cache.all_guilds();
    guilds
        .into_iter()
        .map(|guild| guild.to_partial_guild().unwrap().name)
        .collect()
}

struct Handler {
    listeners: Arc<Mutex<Vec<(ChannelId, String)>>>,
}

impl EventHandler for Handler {
    fn message(&self, _ctx: Context, message: Message) {
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

        let is_owner: bool = message.author.id == 90_927_967_651_262_464;

        if message.content == "!events" {
            let events = events::get_events();

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
            let _ = message.channel_id.say(food::check_for(item));
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

fn read_listeners() -> Vec<(ChannelId, String)> {
    let mut listeners_string: String = String::new();
    let _ = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .open("listeners.txt")
        .expect("No listeners file")
        .read_to_string(&mut listeners_string);

    let mut listeners: Vec<(ChannelId, String)> = vec![];

    for line in listeners_string.split('\n') {
        if line == "" {
            continue;
        }
        let sections: Vec<&str> = line.split(' ').collect();
        let id = ChannelId(
            sections[0]
                .parse::<u64>()
                .expect("Couldn't parse channel id"),
        );
        let food: String = sections[1..].join(" ").to_string();
        listeners.push((id, food));
    }

    listeners
}

fn save_listeners(pairs: &[(ChannelId, String)]) {
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
            let _ = channel.say(food::check_for(&food));
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
