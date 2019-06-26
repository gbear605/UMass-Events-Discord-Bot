extern crate chrono;
extern crate futures;
extern crate reqwest;
extern crate select;
extern crate serenity;
extern crate telegram_bot;
extern crate tokio;
extern crate tokio_core;

// For discord
use chrono::Timelike;
use serenity::client::Client;
use serenity::http::GuildPagination;
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

// For telegram
use futures::{future::lazy, Stream};
use telegram_bot::*;

use food::FoodStore;
use rooms::RoomStore;

// For commandline args
use std::env;

// Allow openssl crosscompiling to work
extern crate openssl_probe;

use std::time::Duration;

use tokio_core::reactor::Core;

mod events;
mod food;
mod rooms;

// Checks that a message successfully sent; if not, then logs why to stdout.
fn check_msg<T>(result: serenity::Result<T>) {
    if let Err(why) = result {
        println!("Discord error: {:?}", why);
    }
}

// Represents either a discord channel or a telegram message chat
#[derive(Debug, Clone)]
enum Channel {
    Discord(ChannelId),
    Telegram(TelegramChannel),
}

#[derive(Debug, Clone)]
enum TelegramChannel {
    ChannelId(telegram_bot::types::ChannelId),
    ChatMessage(telegram_bot::types::MessageChat),
}

impl TelegramChannel {
    fn to_chat_ref(&self) -> ChatRef {
        match self {
            TelegramChannel::ChannelId(id) => id.to_chat_ref(),
            TelegramChannel::ChatMessage(msg) => msg.to_chat_ref(),
        }
    }
}

impl Channel {
    fn send_message(
        &self,
        message: String,
        telegram_token: Option<&str>,
        sent_from_telegram: bool,
        ctx: Option<&Context>,
    ) {
        match self {
            Channel::Discord(channel_id) => {
                check_msg(channel_id.say(&(ctx.unwrap()).http, message));
            }
            Channel::Telegram(channel_id) => {
                if telegram_token.is_none() {
                    println!("Trying to send message to telegram when not connected to telegram!");
                    return;
                }
                let telegram_token = telegram_token.unwrap();
                let core = Core::new().unwrap();
                let handle = core.handle();
                let api = Api::configure(telegram_token).build(handle).unwrap();
                let send_message = channel_id.to_chat_ref().text(message);
                if sent_from_telegram {
                    api.spawn(send_message);
                } else {
                    tokio::runtime::current_thread::Runtime::new()
                        .unwrap()
                        .block_on(lazy(|| api.send(send_message)))
                        .unwrap();
                }
            }
        }
    }
}

impl std::fmt::Display for Channel {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            Channel::Discord(id) => id.fmt(f),
            Channel::Telegram(id) => write!(f, "{:?}", id),
        }
    }
}

// Get the discord token file from memory
fn load_discord_token() -> String {
    let mut token = String::new();
    let _ = File::open("discord_token")
        .expect("No token file")
        .read_to_string(&mut token);
    token
}

// Get the telegram token file from memory
fn load_telegram_token() -> String {
    let mut token = String::new();
    let _ = File::open("telegram_token")
        .expect("No token file")
        .read_to_string(&mut token);
    token.trim().to_string()
}

// Login to Discord and connect
fn login_discord(
    listeners: Arc<Mutex<Vec<(Channel, String)>>>,
    telegram_token: Option<String>,
    food_store: FoodStore,
    room_store: RoomStore,
) -> Client {
    Client::new(
        load_discord_token().trim(),
        Handler {
            listeners,
            telegram_token,
            food_store,
            room_store,
        },
    )
    .expect("Error creating client")
}

fn get_guilds(ctx: Context) -> Vec<String> {
    let guilds = ctx
        .http
        .get_guilds(&GuildPagination::After(GuildId(0)), 100)
        .unwrap();
    guilds.into_iter().map(|guild| guild.name).collect()
}

struct Handler {
    listeners: Arc<Mutex<Vec<(Channel, String)>>>,
    telegram_token: Option<String>,
    food_store: FoodStore,
    room_store: RoomStore,
}

enum UserId {
    Discord(serenity::model::id::UserId),
    Telegram(telegram_bot::types::UserId),
}

impl std::fmt::Display for UserId {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            UserId::Discord(id) => id.fmt(f),
            UserId::Telegram(id) => id.fmt(f),
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

    fn from_telegram_message(user: telegram_bot::types::User) -> User {
        let full_name = match (user.last_name, user.username) {
            (None, None) => user.first_name.clone(),
            (None, Some(username)) => format!("{} ({})", user.first_name, username),
            (Some(last_name), None) => format!("{} {}", user.first_name, last_name),
            (Some(last_name), Some(username)) => {
                format!("{} {} ({})", user.first_name, last_name, username)
            }
        };

        User {
            id: UserId::Telegram(user.id),
            unique_name: full_name,
            is_owner: user.id == telegram_bot::types::UserId::new(698_919_547),
        }
    }

    fn is_self(&self, ctx: Option<&Context>) -> serenity::Result<bool> {
        match self.id {
            UserId::Discord(id) => {
                Ok((&ctx.unwrap()).http.get_current_application_info()?.id == id)
            }
            UserId::Telegram(_id) => {
                Ok(false)
                // isn't needed, since it doesn't get messages from itself, unlike with Discord
            }
        }
    }
}

fn handle_message(
    content: String,
    author: User,
    channel: Channel,
    listeners: Arc<Mutex<Vec<(Channel, String)>>>,
    telegram_api: Option<&str>,
    started_by_telegram: bool,
    food_store: FoodStore,
    room_store: RoomStore,
    ctx: Option<&Context>,
) {
    println!("{}: {} says: {}", author.unique_name, author.id, content);
    if !content.starts_with('!') && !content.starts_with('/') {
        // It's not a command, so we don't care about it
        return;
    }

    // We don't want to respond to ourselves
    // For instance, this would cause issues with !help
    if let Ok(val) = author.is_self(ctx) {
        if val {
            return;
        }
    }

    if content == "!events" || content == "/events" {
        let events = events::get_events();

        // Intro
        channel.send_message(
            "Today's events are:".to_string(),
            telegram_api,
            started_by_telegram,
            ctx,
        );

        for event in events {
            channel.send_message(event.format(), telegram_api, started_by_telegram, ctx);
        }
    } else if content.starts_with("!menu ") || content.starts_with("/menu ") {
        let item: &str = &content[6..];

        channel.send_message(
            food::check_for(item, &food_store),
            telegram_api,
            started_by_telegram,
            ctx,
        );
    } else if content.starts_with("!register ") || content.starts_with("/register ") {
        let item: String = content[10..].to_string();
        listeners
            .lock()
            .unwrap()
            .deref_mut()
            .push((channel.clone(), item.clone()));
        save_listeners(listeners.lock().unwrap().deref_mut());
        channel.send_message(
            format!("Will check for {}", item).to_string(),
            telegram_api,
            started_by_telegram,
            ctx,
        );

        // We also want to check if the food is being served today
        channel.send_message(
            food::check_for(&item, &food_store),
            telegram_api,
            started_by_telegram,
            ctx,
        );
    } else if content == "!help" || content == "/help" {
        match channel {
            Channel::Discord(_) => {
                channel.send_message(
                    "```!menu [food name]     | tells you where that food is being served \
                     today```"
                        .to_string(),
                    telegram_api,
                    started_by_telegram,
                    ctx,
                );

                channel.send_message(
                    "```!register [food name] | schedules it to tell you each day where that \
                     food is being served that day```"
                        .to_string(),
                    telegram_api,
                    started_by_telegram,
                    ctx,
                );
            }
            Channel::Telegram(_) => {
                channel.send_message(
                    "/menu [food name] => tells you where that food is being served today"
                        .to_string(),
                    telegram_api,
                    started_by_telegram,
                    ctx,
                );

                channel.send_message(
                    "/register [food name] => schedules it to tell you each day where that food is being served that day"
                        .to_string(),
                    telegram_api,
                    started_by_telegram,
                    ctx
                );
            }
        }
    } else if content.starts_with("!room ") || content.starts_with("/room ") {
        let room: String = content[6..].to_string();

        let room_store = room_store.lock().unwrap();

        if !room_store.contains_key(&room) {
            channel.send_message(
                format!("Room {} not found on SPIRE", room).to_string(),
                telegram_api,
                started_by_telegram,
                ctx,
            )
        } else {
            channel.send_message(
                format!("Room {}: ", room).to_string(),
                telegram_api,
                started_by_telegram,
                ctx,
            );

            let sections: Vec<rooms::Section> = room_store.get(&room).unwrap().to_vec();
            println!("{:?}", sections);
            for section in sections {
                thread::sleep(Duration::from_millis(100));

                println!("{:?}", section);
                channel.send_message(
                    format!("{:?}", section).to_string(),
                    telegram_api,
                    started_by_telegram,
                    ctx,
                );
            }
        }
    } else if content == "!run" || content == "/run" {
        channel.send_message(
            "Checking for preregistered foods".to_string(),
            telegram_api,
            started_by_telegram,
            ctx,
        );
        check_for_foods(
            &listeners,
            telegram_api,
            started_by_telegram,
            &food_store,
            ctx,
        );
    } else if (content.starts_with("!quit") || content.starts_with("/quit")) && author.is_owner {
        channel.send_message(
            "UMass Bot Quitting".to_string(),
            telegram_api,
            started_by_telegram,
            ctx,
        );
        std::process::exit(0);
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
        let food_store = Arc::clone(&self.food_store);
        let room_store = Arc::clone(&self.room_store);
        let listeners = Arc::clone(&self.listeners);
        let author = User::from_discord_message(&message);
        let content = message.content.clone();
        let channel = Channel::Discord(message.channel_id);

        let telegram_token_clone: String;
        let telegram_token = match self.telegram_token.clone() {
            None => None,
            Some(s) => {
                telegram_token_clone = s.clone();
                Some(telegram_token_clone.as_str())
            }
        };

        handle_message(
            content,
            author,
            channel,
            listeners,
            telegram_token,
            false,
            food_store,
            room_store,
            Some(&ctx),
        );
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
        let app = sections[0];
        let id = if app == "discord" {
            Channel::Discord(ChannelId(
                sections[1]
                    .parse::<u64>()
                    .expect("Couldn't parse channel id"),
            ))
        } else {
            Channel::Telegram(TelegramChannel::ChannelId(
                telegram_bot::types::ChannelId::from(
                    sections[1]
                        .parse::<i64>()
                        .expect("Couldn\'t parse channel id"),
                ),
            ))
        };

        let food: String = sections[2..].join(" ").to_string();
        listeners.push((id, food));
    }

    listeners
}

fn save_listeners(pairs: &[(Channel, String)]) {
    let mut listeners_string: String = String::new();
    pairs.iter().for_each(|x| {
        listeners_string = match *x {
            (Channel::Discord(ref id), ref food) => {
                format!("{}discord {} {}\n", listeners_string, id, food)
            }
            (Channel::Telegram(ref id), ref food) => {
                format!("{}telegram {:?} {}\n", listeners_string, id, food)
            }
        };
    });

    let listeners_string = listeners_string.trim();

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

fn check_for_foods(
    listeners: &Arc<Mutex<Vec<(Channel, String)>>>,
    telegram_api: Option<&str>,
    started_by_telegram: bool,
    store: &FoodStore,
    ctx: Option<&Context>,
) {
    listeners
        .lock()
        .unwrap()
        .to_vec()
        .into_iter()
        .for_each(|(channel, food)| {
            println!("Checking on {:?} for {}", channel, food);
            channel.send_message(
                food::check_for(&food, &store),
                telegram_api,
                started_by_telegram,
                ctx,
            );
        });
}

fn run_discord_client(mut client: Client) {
    if let Err(why) = client.start() {
        println!("Discord client error: {:?}", why);
    }
}

fn run_telegram_client(
    telegram_token: &str,
    listeners: Arc<Mutex<Vec<(Channel, String)>>>,
    food_store: FoodStore,
    room_store: RoomStore,
) {
    let mut core = Core::new().unwrap();
    let handle = core.handle();

    let api = Api::configure(telegram_token.clone())
        .build(handle)
        .unwrap();

    let stream = api.stream().then(|mb_update| {
        let res: Result<Result<Update, Error>, ()> = Ok(mb_update);
        res
    });

    // Fetch new updates via long poll method
    let future = stream.for_each(move |update| {
        match update {
            Ok(update) => {
                // If the received update contains a new message...
                if let UpdateKind::Message(message) = update.kind {
                    println!("Some message {:?}", message);
                    if let MessageKind::Text { ref data, .. } = message.kind {
                        handle_message(
                            data.to_string(),
                            User::from_telegram_message(message.from),
                            Channel::Telegram(TelegramChannel::ChatMessage(message.chat)),
                            Arc::clone(&listeners),
                            Some(telegram_token),
                            true,
                            Arc::clone(&food_store),
                            Arc::clone(&room_store),
                            None,
                        );
                    }
                } else {
                    println!("Some update {:?}", update);
                }
            }
            Err(e) => {
                println!("Some error {:?}", e);
            }
        }

        Ok(())
    });

    core.run(future).unwrap();
}

fn main() {
    // Allow openssl crosscompiling to work
    openssl_probe::init_ssl_cert_env_vars();

    // Decide whether or not to run telegram or discord connection
    // This is useful for testing
    let args: Vec<String> = env::args().collect();
    let run_telegram = !args.contains(&"--no-telegram".to_string());
    let run_discord = !args.contains(&"--no-discord".to_string());

    if !run_discord && !run_telegram {
        return;
    }

    let listeners: Arc<Mutex<Vec<(Channel, String)>>> = Arc::new(Mutex::new(read_listeners()));

    let food_store: FoodStore =
        Arc::new(Mutex::new((food::get_date(), food::get_menus_no_cache())));
    let rooms_store: RoomStore = Arc::new(Mutex::new(rooms::load_sections_map()));

    let telegram_token = if run_telegram {
        Some(load_telegram_token())
    } else {
        None
    };

    // Setup discord
    let discord_client = if run_discord {
        let client = login_discord(
            Arc::clone(&listeners),
            telegram_token.clone(),
            Arc::clone(&food_store),
            Arc::clone(&rooms_store),
        );

        let owners = match client.cache_and_http.http.get_current_application_info() {
            Ok(info) => {
                let mut set = HashSet::new();
                set.insert(info.owner.id);

                set
            }
            Err(why) => panic!("Couldn't get application info: {:?}", why),
        };

        println!("Owners: {:?}", owners);

        Some(client)
    } else {
        None
    };

    // Listeners loop
    let listeners_clone = Arc::clone(&listeners);
    let food_store_clone = Arc::clone(&food_store);
    let telegram_token_clone = telegram_token.clone();
    thread::spawn(move || {
        let listeners = listeners_clone;
        let food_store = food_store_clone;
        loop {
            let telegram_token = telegram_token_clone.clone();
            println!("Seconds till scheduled: {:?}", get_time_till_scheduled());
            thread::sleep(get_time_till_scheduled());
            println!("Checking for foods now!");
            check_for_foods(
                &listeners,
                Some(&telegram_token.unwrap()),
                false,
                &food_store,
                None, //TODO: this needs to be Some for Discord
            );
        }
    });

    if run_telegram && run_discord {
        // Start discord loop
        thread::spawn(move || {
            run_discord_client(discord_client.unwrap());
        });

        run_telegram_client(&telegram_token.unwrap(), listeners, food_store, rooms_store);
    } else if run_telegram {
        run_telegram_client(&telegram_token.unwrap(), listeners, food_store, rooms_store);
    } else if run_discord {
        run_discord_client(discord_client.unwrap());
    }
}
