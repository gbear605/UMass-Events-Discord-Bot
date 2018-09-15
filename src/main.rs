extern crate chrono;
extern crate reqwest;
extern crate select;
#[macro_use]
extern crate serenity;

use chrono::Timelike;
use serenity::client::Client;
use serenity::framework::StandardFramework;
use serenity::http;
use serenity::http::{get_guilds as other_get_guilds, GuildPagination};
use serenity::model::channel::Message;
use serenity::model::event::ResumedEvent;
use serenity::model::id::ChannelId;
use serenity::model::id::GuildId;
use serenity::model::prelude::Ready;
use serenity::prelude::*;

use std::collections::HashSet;

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

mod events;
mod food;

// Checks that a message successfully sent; if not, then logs why to stdout.
fn check_msg(result: serenity::Result<Message>) {
    if let Err(why) = result {
        println!("Error sending message: {:?}", why);
    }
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
fn login(listeners: Arc<Mutex<Vec<(ChannelId, String)>>>) -> Client {
    Client::new(load_token().trim(), Handler { listeners }).expect("Error creating client")
}

fn get_guilds() -> Vec<String> {
    let guilds = other_get_guilds(&GuildPagination::After(GuildId(0)), 100).unwrap();
    guilds.into_iter().map(|guild| guild.name).collect()
}

struct Handler {
    listeners: Arc<Mutex<Vec<(ChannelId, String)>>>,
}

impl EventHandler for Handler {
    fn ready(&self, _: Context, ready: Ready) {
        println!("Connected to Discord as {}", ready.user.name);
        println!("Connected to servers: {}", get_guilds().join(", "));
    }

    fn resume(&self, _: Context, _: ResumedEvent) {
        println!("Resumed");
    }

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
            check_msg(message.channel_id.say("Today's events are:".to_string()));

            events
                .iter()
                .for_each(|event| check_msg(message.channel_id.say(event.format().to_string())));
        } else if message.content.starts_with("!menu ") {
            let item: &str = &message.content[6..];

            check_msg(
                message
                    .channel_id
                    .say(format!("Checking for {}\n", item).to_string()),
            );
            check_msg(message.channel_id.say(food::check_for(item)));
        } else if message.content.starts_with("!register ") {
            let item: String = message.content[10..].to_string();
            listeners
                .lock()
                .unwrap()
                .deref_mut()
                .push((message.channel_id, item.clone()));
            save_listeners(listeners.lock().unwrap().deref_mut());
            check_msg(
                message
                    .channel_id
                    .say(format!("Will check for {}", item).to_string()),
            );
        } else if message.content == "!help" {
            check_msg(message.channel_id.say("UMass Bot help:"));
            check_msg(message.channel_id.say(
                "```!menu [food name]     | tells you where that food is being served \
                 today```",
            ));
            check_msg(message.channel_id.say(
                "```!register [food name] | schedules it to tell you each day where that \
                 food is being served that day```",
            ));
        } else if message.content == "!run" {
            check_msg(message.channel_id.say("Checking for preregistered foods"));
            check_for_foods(&listeners);
        } else if message.content.starts_with("!quit") && is_owner {
            check_msg(message.channel_id.say("UMass Bot Quitting"));
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

// Runs at 6 AM in summer or 5 AM in winter
fn get_time_till_scheduled() -> std::time::Duration {
    let current_time_utc = chrono::prelude::Utc::now();
    let current_time: chrono::DateTime<chrono::offset::FixedOffset> = chrono::DateTime::from_utc(
        current_time_utc.naive_utc(),
        chrono::offset::FixedOffset::west(4 * 60 * 60),
        // Four hours west of the date line
        // Four instead of five because 5am/6am is a better default than 6am/7am
    );
    let next_run_date = if current_time.time().hour() < 6
        || (current_time.hour() == 6 && current_time.minute() < 5)
    {
        // We want to do it today (in UTC) if it is still yesterday in Eastern Time
        current_time
    } else {
        current_time + chrono::Duration::days(1)
    }.date();

    let next_run = next_run_date.and_hms(6, 5, 0);

    (next_run - current_time).to_std().unwrap()
}

fn check_for_foods(listeners: &Arc<Mutex<Vec<(ChannelId, String)>>>) {
    listeners
        .lock()
        .unwrap()
        .to_vec()
        .into_iter()
        .for_each(|(channel, food)| {
            println!("Checking on {:?} for {}", channel, food);
            check_msg(channel.say(food::check_for(&food)));
        });
}

fn main() {
    // Allow openssl crosscompiling to work
    openssl_probe::init_ssl_cert_env_vars();

    let listeners: Arc<Mutex<Vec<(ChannelId, String)>>> = Arc::new(Mutex::new(read_listeners()));
    let mut client = login(Arc::clone(&listeners));

    let owners = match http::get_current_application_info() {
        Ok(info) => {
            let mut set = HashSet::new();
            set.insert(info.owner.id);

            set
        }
        Err(why) => panic!("Couldn't get application info: {:?}", why),
    };

    println!("{:?}", owners);

    client.with_framework(
        StandardFramework::new()
            .configure(|c| c.owners(owners).prefix("!"))
            .command(
                "guilds",
                |c| c.cmd(guildsCommand), /*.owners_only(true)*/
            ),
    );

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

    if let Err(why) = client.start() {
        println!("Client error: {:?}", why);
    }
}

command!(guildsCommand(_context, message) {
    check_msg(message.reply(&format!("Guilds: {}", get_guilds().join(", "))))
});
