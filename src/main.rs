extern crate chrono;
extern crate futures;
extern crate reqwest;
extern crate select;
extern crate serenity;
extern crate telegram_bot_fork;
extern crate tokio;

// For discord
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

// For telegram
use futures::{future::lazy, Stream};
use telegram_bot_fork::*;

use food::FoodStore;

// Allow openssl crosscompiling to work
extern crate openssl_probe;

mod events;
mod food;

enum Message {
    Discord(serenity::model::channel::Message),
    //Telegram(telegram_bot_fork::types::requests::SendMessage),
}

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
    ChannelId(telegram_bot_fork::types::ChannelId),
    ChatMessage(telegram_bot_fork::types::MessageChat),
}

impl TelegramChannel {
    fn to_chat_ref(&self) -> ChatRef {
        match self {
            TelegramChannel::ChannelId(id) => id.to_chat_ref(),
            TelegramChannel::ChatMessage(msg) => msg.to_chat_ref(),
        }
    }

    fn to_id(&self) -> i64 {
        match self.to_chat_ref() {
            ChatRef::Id(id) => id.into(),
            ChatRef::ChannelUsername(ref user) => panic!("Can't handle channel username"),
        }
    }
}

impl Channel {
    fn send_message(&self, message: String, telegram_token: &str, sent_from_telegram: bool) {
        match self {
            Channel::Discord(channel_id) => {
                check_msg(channel_id.say(message).map(|msg| Message::Discord(msg)));
            }
            Channel::Telegram(channel_id) => {
                let api = Api::new(telegram_token).unwrap();
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
fn login_discord(listeners: Arc<Mutex<Vec<(Channel, String)>>>, store: FoodStore) -> Client {
    Client::new(
        load_discord_token().trim(),
        Handler {
            listeners,
            telegram_token: load_telegram_token(),
            store: store,
        },
    )
    .expect("Error creating client")
}

fn get_guilds() -> Vec<String> {
    let guilds = other_get_guilds(&GuildPagination::After(GuildId(0)), 100).unwrap();
    guilds.into_iter().map(|guild| guild.name).collect()
}

struct Handler {
    listeners: Arc<Mutex<Vec<(Channel, String)>>>,
    telegram_token: String,
    store: FoodStore,
}

enum UserId {
    Discord(serenity::model::id::UserId),
    Telegram(telegram_bot_fork::types::UserId),
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
    // In telegram, name is "first_name";
    // In discord, name is "name"
    // Neither of these are constant for a user or unique
    name: String,
    // Whether this user is an admin of the bot
    is_owner: bool,
}

impl User {
    fn from_discord_message(message: &serenity::model::channel::Message) -> User {
        User {
            id: UserId::Discord(message.author.id),
            unique_name: format!("{}#{}", message.author.name, message.author.discriminator),
            name: message.author.name.clone(),
            is_owner: message.author.id == 90_927_967_651_262_464,
        }
    }

    fn from_telegram_message(user: telegram_bot_fork::types::User) -> User {
        let full_name = match (user.last_name, user.username) {
            (None, None) => user.first_name.clone(),
            (None, Some(username)) => format!("{} ({})", user.first_name, username),
            (Some(last_name), None) => format!("{} {}", user.first_name, last_name),
            (Some(last_name), Some(username)) => {
                format!("{} {} ({})", user.first_name, last_name, username)
            }
        };

        let name = user.first_name;

        User {
            id: UserId::Telegram(user.id),
            unique_name: full_name,
            name: name,
            is_owner: user.id.to_string() == "698919547",
        }
    }

    fn is_self(&self) -> serenity::Result<bool> {
        match self.id {
            UserId::Discord(id) => {
                Ok(serenity::http::raw::get_current_application_info()?.id == id)
            }
            UserId::Telegram(_id) => {
                Ok(false) // TODO
            }
        }
    }
}

fn handle_message<'a>(
    content: String,
    author: User,
    channel: Channel,
    listeners: Arc<Mutex<Vec<(Channel, String)>>>,
    telegram_api: &str,
    started_by_telegram: bool,
    store: FoodStore,
) -> Option<SendMessage<'a>> {
    if !content.starts_with('!') {
        // It's not a command, so we don't care about it
        return None;
    }

    // We don't want to respond to ourselves
    // For instance, this would cause issues with !help
    if let Ok(val) = author.is_self() {
        if val {
            return None;
        }
    }

    println!("{}: {} says: {}", author.unique_name, author.id, content);

    if content == "!events" {
        let events = events::get_events();

        // Intro
        channel.send_message(
            "Today's events are:".to_string(),
            telegram_api,
            started_by_telegram,
        );

        for event in events {
            channel.send_message(event.format(), telegram_api, started_by_telegram);
        }
    } else if content.starts_with("!menu ") {
        let item: &str = &content[6..];

        channel.send_message(
            food::check_for(item, &store),
            telegram_api,
            started_by_telegram,
        );
    } else if content.starts_with("!register ") {
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
        );
    } else if content == "!help" {
        match channel {
            Channel::Discord(_) => {
                channel.send_message(
                    "```!menu [food name]     | tells you where that food is being served \
                     today```"
                        .to_string(),
                    telegram_api,
                    started_by_telegram,
                );

                channel.send_message(
                    "```!register [food name] | schedules it to tell you each day where that \
                     food is being served that day```"
                        .to_string(),
                    telegram_api,
                    started_by_telegram,
                );
            }
            Channel::Telegram(_) => {
                channel.send_message(
                    "!menu [food name] => tells you where that food is being served today"
                        .to_string(),
                    telegram_api,
                    started_by_telegram,
                );

                channel.send_message(
                    "!register [food name] => schedules it to tell you each day where that food is being served that day"
                        .to_string(),
                    telegram_api,
                    started_by_telegram,
                );
            }
        }
    } else if content == "!run" {
        channel.send_message(
            "Checking for preregistered foods".to_string(),
            telegram_api,
            started_by_telegram,
        );
        check_for_foods(&listeners, telegram_api, started_by_telegram, &store);
    } else if content.starts_with("!quit") && author.is_owner {
        channel.send_message(
            "UMass Bot Quitting".to_string(),
            telegram_api,
            started_by_telegram,
        );
        std::process::exit(0);
    }
    return None;
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
        let store = Arc::clone(&self.store);
        let listeners = Arc::clone(&self.listeners);
        let author = User::from_discord_message(&message);
        let content = message.content.clone();
        let channel = Channel::Discord(message.channel_id);
        handle_message(
            content,
            author,
            channel,
            listeners,
            &self.telegram_token,
            false,
            store,
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
                telegram_bot_fork::types::ChannelId(
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
    pairs.into_iter().for_each(|x| {
        listeners_string = match *x {
            (Channel::Discord(ref id), ref food) => {
                format!("{}discord {} {}\n", listeners_string, id, food)
            }
            (Channel::Telegram(ref id), ref food) => {
                format!("{}telegram {} {}\n", listeners_string, id.to_id(), food)
            }
        };
    });

    listeners_string.trim();

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
    telegram_api: &str,
    started_by_telegram: bool,
    store: &FoodStore,
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
            );
        });
}

fn main() {
    // Allow openssl crosscompiling to work
    openssl_probe::init_ssl_cert_env_vars();

    let listeners: Arc<Mutex<Vec<(Channel, String)>>> = Arc::new(Mutex::new(read_listeners()));

    let store: FoodStore = Arc::new(Mutex::new((food::get_date(), food::get_menus_no_cache())));

    // Setup discord
    let mut discord_client = login_discord(Arc::clone(&listeners), Arc::clone(&store));
    let owners = match http::get_current_application_info() {
        Ok(info) => {
            let mut set = HashSet::new();
            set.insert(info.owner.id);

            set
        }
        Err(why) => panic!("Couldn't get application info: {:?}", why),
    };

    println!("Owners: {:?}", owners);

    let telegram_token = load_telegram_token();

    // Listeners loop
    let listeners_clone = Arc::clone(&listeners);
    let store_clone = Arc::clone(&store);
    let telegram_token_clone = telegram_token.clone();
    thread::spawn(move || {
        let listeners = listeners_clone;
        let store = store_clone;
        let telegram_token = telegram_token_clone;
        loop {
            println!("Seconds till scheduled: {:?}", get_time_till_scheduled());
            thread::sleep(get_time_till_scheduled());
            println!("Checking for foods now!");
            check_for_foods(&listeners, &telegram_token, false, &store);
        }
    });

    // Start telegram loop
    thread::spawn(move || {
        if let Err(why) = discord_client.start() {
            println!("Discord client error: {:?}", why);
        }
    });

    tokio::runtime::current_thread::Runtime::new()
        .unwrap()
        .block_on(lazy(|| {
            let api = Api::new(telegram_token.clone()).unwrap();

            let stream = api.stream().then(|mb_update| {
                let res: Result<Result<Update, Error>, ()> = Ok(mb_update);
                res
            });

            // Fetch new updates via long poll method
            stream.for_each(move |update| {
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
                                    &telegram_token,
                                    true,
                                    Arc::clone(&store),
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
            })
        }))
        .unwrap();
}
