extern crate chrono;
extern crate futures;
extern crate openssl_probe;
extern crate reqwest;
extern crate select;
extern crate serenity;
extern crate tokio;
extern crate tokio_core;

// For discord
use chrono::Timelike;
use serenity::client::Client;
use serenity::http::GuildPagination;
use serenity::model::event::ResumedEvent;
use serenity::model::id::ChannelId;
use serenity::model::id::GuildId;
use serenity::model::id::UserId;
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

// Checks that a message successfully sent; if not, then logs why to stdout.
fn check_msg<T>(result: serenity::Result<T>) {
    if let Err(why) = result {
        println!("Discord error: {:?}", why);
    }
}

fn send_message(channel_id: ChannelId, message: String, ctx: &Context) {
    check_msg(channel_id.say(&ctx.http, message));
}

fn check_food(food: String) -> String {
    let client = reqwest::Client::new();
    client
        .get("http://localhost:8000/food/")
        .query(&[("food", food)])
        .send()
        .unwrap()
        .text()
        .unwrap()
}

// Get the discord token file from memory
fn load_discord_token() -> String {
    let mut token = String::new();
    let _ = File::open("discord_token")
        .expect("No token file")
        .read_to_string(&mut token);
    token
}

fn get_guilds(ctx: Context) -> Vec<String> {
    let guilds = ctx
        .http
        .get_guilds(&GuildPagination::After(GuildId(0)), 100)
        .unwrap();
    guilds.into_iter().map(|guild| guild.name).collect()
}

struct Handler {
    listeners: Arc<Mutex<Vec<(ChannelId, String)>>>,
}

struct User {
    id: UserId,
    // uniqueName is "name#discriminator"
    // It is not constant for a user
    unique_name: String,
    // Whether this user is an admin of the bot
    is_owner: bool,
}

impl User {
    fn from_discord_message(message: &serenity::model::channel::Message) -> User {
        User {
            id: message.author.id,
            unique_name: format!("{}#{}", message.author.name, message.author.discriminator),
            is_owner: message.author.id == 90_927_967_651_262_464,
        }
    }

    fn is_self(&self, ctx: &Context) -> serenity::Result<bool> {
        Ok(ctx.http.get_current_application_info()?.id == self.id)
    }
}

impl EventHandler for Handler {
    fn ready(&self, ctx: Context, ready: Ready) {
        println!("Connected to Discord as {}", ready.user.name);
        println!("Connected to servers: {}", get_guilds(ctx).join(", "));
    }

    fn resume(&self, _: Context, _: ResumedEvent) {
        println!("Resumed");
    }

    // Discord specific
    fn message(&self, ctx: Context, message: serenity::model::channel::Message) {
        let listeners = Arc::clone(&self.listeners);
        let author = User::from_discord_message(&message);
        let content = message.content.clone();
        let channel = message.channel_id;

        println!("{}: {} says: {}", author.unique_name, author.id, content);
        if !content.starts_with('!') {
            // It's not a command, so we don't care about it
            return;
        }

        // We don't want to respond to ourselves
        // For instance, this would cause issues with !help
        if let Ok(val) = author.is_self(&ctx) {
            if val {
                return;
            }
        }

        if content.starts_with("!menu ") {
            let item: &str = &content[6..];

            let response = check_food(item.to_string());

            send_message(channel, format!("{}", response), &ctx);
        } else if content.starts_with("!echo ") {
            let input: String = content[5..].to_string();

            let client = reqwest::Client::new();
            let mut res = client
                .get("http://localhost:8000/echo/")
                .query(&[("input", input)])
                .send()
                .unwrap();

            send_message(channel, format!("{}", res.text().unwrap()), &ctx);
        } else if content.starts_with("!register ") {
            let item: String = content[10..].to_string();
            listeners
                .lock()
                .unwrap()
                .deref_mut()
                .push((channel.clone(), item.clone()));
            save_listeners(listeners.lock().unwrap().deref_mut());
            send_message(
                channel,
                format!("Will check for {}", item).to_string(),
                &ctx,
            );

            let response = check_food(item.to_string());

            send_message(channel, format!("{}", response), &ctx);
        } else if content == "!help" {
            send_message(
                channel,
                "```!menu [food name]     | tells you where that food is being served \
                 today```"
                    .to_string(),
                &ctx,
            );

            send_message(
                channel,
                "```!register [food name] | schedules it to tell you each day where that \
                 food is being served that day```"
                    .to_string(),
                &ctx,
            );
        } else if content.starts_with("!room ") {
            let room: String = content[6..].to_string();

            let client = reqwest::Client::new();
            let res = client
                .get("http://localhost:8000/room/")
                .query(&[("room", room)])
                .send()
                .unwrap();

            if res.status().is_success() {
                send_message(channel, format!("Some classes meet in that room",), &ctx);
            } else {
                send_message(channel, format!("No classes meet in that room",), &ctx);
            }
        } else if content == "!run" {
            send_message(
                channel,
                "Checking for preregistered foods".to_string(),
                &ctx,
            );
            check_for_foods(&listeners, &ctx);
        } else if (content.starts_with("!quit")) && author.is_owner {
            send_message(channel, "UMass Bot Quitting".to_string(), &ctx);
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
        .open("discord_listeners.txt")
        .expect("No discord listeners file")
        .read_to_string(&mut listeners_string);

    let mut listeners: Vec<(ChannelId, String)> = vec![];

    for line in listeners_string.split('\n') {
        if line == "" {
            continue;
        }
        let sections: Vec<&str> = line.split(' ').collect();
        let app = sections[0];
        if app == "discord" {
            let id = ChannelId(
                sections[1]
                    .parse::<u64>()
                    .expect("Couldn't parse channel id"),
            );

            let food: String = sections[2..].join(" ").to_string();
            listeners.push((id, food));
        }
    }

    listeners
}

fn save_listeners(pairs: &[(ChannelId, String)]) {
    let mut listeners_string: String = String::new();
    pairs.iter().for_each(|x| {
        listeners_string = match *x {
            (ref id, ref food) => format!("{}discord {} {}\n", listeners_string, id, food),
        };
    });

    let listeners_string = listeners_string.trim();

    let _ = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(true)
        .open("discord_listeners.txt")
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

fn check_for_foods(listeners: &Arc<Mutex<Vec<(ChannelId, String)>>>, ctx: &Context) {
    listeners
        .lock()
        .unwrap()
        .to_vec()
        .into_iter()
        .for_each(|(channel, food)| {
            println!("Checking on {:?} for {}", channel, food);
            let response = check_food(food);

            send_message(channel, format!("{}", response), ctx);
        });
}

fn main() {
    // Allow openssl crosscompiling to work
    openssl_probe::init_ssl_cert_env_vars();

    let listeners: Arc<Mutex<Vec<(ChannelId, String)>>> = Arc::new(Mutex::new(read_listeners()));

    // Setup discord
    let mut client = Client::new(load_discord_token().trim(), Handler { listeners })
        .expect("Error creating client");

    let owners = match client.cache_and_http.http.get_current_application_info() {
        Ok(info) => {
            let mut set = HashSet::new();
            set.insert(info.owner.id);

            set
        }
        Err(why) => panic!("Couldn't get application info: {:?}", why),
    };

    println!("Owners: {:?}", owners);

    // Listeners loop
    /*let listeners_clone = Arc::clone(&listeners);
    thread::spawn(move || {
        let listeners = listeners_clone;
        loop {
            println!("Seconds till scheduled: {:?}", get_time_till_scheduled());
            thread::sleep(get_time_till_scheduled());
            println!("Checking for foods now!");
            check_for_foods(
                &listeners, None, //TODO: this needs to be Some for Discord
            );
        }
    });*/

    if let Err(why) = client.start() {
        println!("Discord client error: {:?}", why);
    }
}
