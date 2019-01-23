extern crate chrono;
extern crate reqwest;
extern crate select;
extern crate serenity;

use chrono::Timelike;
use serenity::client::Client;
use serenity::http;
use serenity::http::{get_guilds as other_get_guilds, GuildPagination};
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

enum Message {
    Discord(serenity::model::channel::Message),
    //Telegram(telegram_bot::types::requests::SendMessage)
}

// Checks that a message successfully sent; if not, then logs why to stdout.
fn check_msg<T>(result: serenity::Result<T>) {
    if let Err(why) = result {
        println!("Discord error: {:?}", why);
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

// Represents either a discord channel or a telegram message chat
#[derive(Debug)]
enum Channel {
    Discord(ChannelId),
    //Telegram(MessageChat),
}

impl Channel {
    fn send_message(&self, message: String) -> serenity::Result<Message> {
        match self {
            Channel::Discord(channel_id) => {
                channel_id.say(message).map(|msg| Message::Discord(msg))
            }
        }
    }
}

impl std::fmt::Display for Channel {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            Channel::Discord(id) => id.fmt(f),
        }
    }
}

impl std::clone::Clone for Channel {
    fn clone(&self) -> Channel {
        match self {
            Channel::Discord(id) => Channel::Discord(id.clone()),
        }
    }
}

// Login to Discord and connect
fn login_discord(listeners: Arc<Mutex<Vec<(Channel, String)>>>) -> Client {
    Client::new(load_token().trim(), Handler { listeners }).expect("Error creating client")
}

fn get_guilds() -> Vec<String> {
    let guilds = other_get_guilds(&GuildPagination::After(GuildId(0)), 100).unwrap();
    guilds.into_iter().map(|guild| guild.name).collect()
}

struct Handler {
    listeners: Arc<Mutex<Vec<(Channel, String)>>>,
}

enum UserId {
    Discord(serenity::model::id::UserId),
}

impl std::fmt::Display for UserId {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            UserId::Discord(id) => id.fmt(f),
        }
    }
}

// Generic user that should work across Discord and Telegram
struct User {
    id: UserId,
    // In telegram, uniqueName is "first_name last_name (username)";
    //  We are only guarenteed to have "first_name" though.
    // In discord, uniqueName is "name#discriminator"
    // Neither of these are constant for a user
    unique_name: String,
    // Whether this user is an admin of the bot
    is_owner: bool,
}

impl User {
    fn from_discord_message(message: &serenity::model::channel::Message) -> User {
        User {
            id: UserId::Discord(message.author.id),
            unique_name: format!("{}#{}", message.author.name, message.author.discriminator),
            is_owner: message.author.id == 90_927_967_651_262_464,
        }
    }

    fn is_self(&self) -> serenity::Result<bool> {
        match self.id {
            UserId::Discord(id) => {
                Ok(serenity::http::raw::get_current_application_info()?.id == id)
            }
        }
    }
}

fn handle_message(
    content: String,
    author: User,
    channel: Channel,
    listeners: Arc<Mutex<Vec<(Channel, String)>>>,
) -> serenity::Result<()> {
    if !content.starts_with('!') {
        // It's not a command, so we don't care about it
        return Ok(());
    }

    // We don't want to respond to ourselves
    // For instance, this would cause issues with !help
    if author.is_self()? {
        return Ok(());
    }

    println!("{}: {} says: {}", author.unique_name, author.id, content);

    if content == "!events" {
        let events = events::get_events();

        // Intro
        channel.send_message("Today's events are:".to_string())?;

        for event in events {
            channel.send_message(event.format())?;
        }
    } else if content.starts_with("!menu ") {
        let item: &str = &content[6..];

        channel.send_message(format!("Checking for {}\n", item).to_string())?;
        channel.send_message(food::check_for(item))?;
    } else if content.starts_with("!register ") {
        let item: String = content[10..].to_string();
        listeners
            .lock()
            .unwrap()
            .deref_mut()
            .push((channel.clone(), item.clone()));
        save_listeners(listeners.lock().unwrap().deref_mut());
        channel.send_message(format!("Will check for {}", item).to_string())?;
    } else if content == "!help" {
        channel.send_message("UMass Bot help:".to_string())?;

        channel.send_message(
            "```!menu [food name]     | tells you where that food is being served \
             today```"
                .to_string(),
        )?;

        channel.send_message(
            "```!register [food name] | schedules it to tell you each day where that \
             food is being served that day```"
                .to_string(),
        )?;
    } else if content == "!run" {
        channel.send_message("Checking for preregistered foods".to_string())?;
        check_for_foods(&listeners);
    } else if content.starts_with("!quit") && author.is_owner {
        channel.send_message("UMass Bot Quitting".to_string())?;
        std::process::exit(0);
    }
    return Ok(());
}

impl EventHandler for Handler {
    fn ready(&self, _: Context, ready: Ready) {
        println!("Connected to Discord as {}", ready.user.name);
        println!("Connected to servers: {}", get_guilds().join(", "));
    }

    fn resume(&self, _: Context, _: ResumedEvent) {
        println!("Resumed");
    }

    // Discord specific
    fn message(&self, _ctx: Context, message: serenity::model::channel::Message) {
        let listeners = Arc::clone(&self.listeners);
        let author = User::from_discord_message(&message);
        let content = message.content.clone();
        let channel = Channel::Discord(message.channel_id);
        check_msg(handle_message(content, author, channel, listeners));
    }
}

fn read_listeners() -> Vec<(Channel, String)> {
    let mut listeners_string: String = String::new();
    let _ = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .open("listeners.txt")
        .expect("No listeners file")
        .read_to_string(&mut listeners_string);

    let mut listeners: Vec<(Channel, String)> = vec![];

    for line in listeners_string.split('\n') {
        if line == "" {
            continue;
        }
        let sections: Vec<&str> = line.split(' ').collect();
        let id = Channel::Discord(ChannelId(
            sections[0]
                .parse::<u64>()
                .expect("Couldn't parse channel id"),
        ));
        let food: String = sections[1..].join(" ").to_string();
        listeners.push((id, food));
    }

    listeners
}

fn save_listeners(pairs: &[(Channel, String)]) {
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
    }
    .date();

    let next_run = next_run_date.and_hms(6, 5, 0);

    (next_run - current_time).to_std().unwrap()
}

fn check_for_foods(listeners: &Arc<Mutex<Vec<(Channel, String)>>>) {
    listeners
        .lock()
        .unwrap()
        .to_vec()
        .into_iter()
        .for_each(|(channel, food)| {
            println!("Checking on {:?} for {}", channel, food);
            check_msg(channel.send_message(food::check_for(&food)));
        });
}

fn main() {
    // Allow openssl crosscompiling to work
    openssl_probe::init_ssl_cert_env_vars();

    let listeners: Arc<Mutex<Vec<(Channel, String)>>> = Arc::new(Mutex::new(read_listeners()));
    let mut discord_client = login_discord(Arc::clone(&listeners));

    let owners = match http::get_current_application_info() {
        Ok(info) => {
            let mut set = HashSet::new();
            set.insert(info.owner.id);

            set
        }
        Err(why) => panic!("Couldn't get application info: {:?}", why),
    };

    println!("{:?}", owners);

    // Listeners loop
    thread::spawn(move || {
        let listeners = listeners;
        loop {
            println!("Seconds till scheduled: {:?}", get_time_till_scheduled());
            thread::sleep(get_time_till_scheduled());
            println!("Checking for foods now!");
            check_for_foods(&listeners);
        }
    });

    if let Err(why) = discord_client.start() {
        println!("Discord client error: {:?}", why);
    }
}
