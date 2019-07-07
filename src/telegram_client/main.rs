extern crate chrono;
extern crate futures;
extern crate hyper;
extern crate openssl_probe;
extern crate select;
extern crate telegram_bot;
extern crate tokio;
extern crate tokio_core;

// For discord
use chrono::Timelike;

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
use futures::Stream;
use telegram_bot::*;

use hyper::rt::{Future, Stream as HyperStream};
use hyper::Client;

use tokio_core::reactor::Core;

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

impl TelegramChannel {
    fn send_message(&self, message: String, api: &Api) {
        let send_message = self.to_chat_ref().text(message);
        api.spawn(send_message);
    }
}

fn send_get(url: String) -> String {
    let url = url.replace(" ", "%20");

    let mutex = Arc::new(Mutex::new(None));

    let mutex_c = mutex.clone();

    let t = thread::spawn(move || {
        tokio::run({
            let client = Client::new();
            dbg!(url.clone());
            client
                .get(url.parse().unwrap())
                .and_then(|res| {
                    // asynchronously concatenate chunks of the body
                    res.into_body().concat2()
                })
                .map_err(|err| {
                    println!("Error: {}", err);
                })
                .and_then(move |body| {
                    *mutex_c.lock().unwrap() =
                        Some(format!("{}", std::str::from_utf8(&body).unwrap()));
                    Ok(())
                })
        });
    });

    t.join().unwrap();
    let x = format!("{}", (mutex.lock().unwrap().as_ref().unwrap()));
    x
}

fn check_food(food: String) -> String {
    send_get(format!("http://localhost:8000/food?food={}", food))
}

// Get the telegram token file from memory
fn load_telegram_token() -> String {
    let mut token = String::new();
    let _ = File::open("telegram_token")
        .expect("No token file")
        .read_to_string(&mut token);
    token.trim().to_string()
}

// Generic user that should work across Discord and Telegram
struct User {
    id: UserId,
    // UniqueName is "first_name last_name (username)";
    //  We are only guarenteed to have "first_name" though.
    // It is not constant for a user
    unique_name: String,
    // Whether this user is an admin of the bot
    is_owner: bool,
}

impl User {
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
            id: user.id,
            unique_name: full_name,
            is_owner: user.id == telegram_bot::types::UserId::new(698_919_547),
        }
    }
}

fn handle_message(
    content: String,
    author: User,
    channel: TelegramChannel,
    listeners: Arc<Mutex<Vec<(TelegramChannel, String)>>>,
    telegram_api: &Api,
) {
    println!("{}: {} says: {}", author.unique_name, author.id, content);
    if !content.starts_with('!') && !content.starts_with('/') {
        // It's not a command, so we don't care about it
        return;
    }

    if content.starts_with("/menu ") {
        let item: &str = &content[6..];

        let response = dbg!(check_food(item.to_string()));

        channel.send_message(format!("{}", response), &telegram_api);
    } else if content.starts_with("/echo ") {
        let input: String = content[6..].to_string();

        let res = send_get(format!("http://localhost:8000/echo?input={}", input));

        channel.send_message(format!("{}", res), &telegram_api);
    } else if content.starts_with("/register ") {
        let item: String = content[10..].to_string();
        listeners
            .lock()
            .unwrap()
            .deref_mut()
            .push((channel.clone(), item.clone()));
        save_listeners(listeners.lock().unwrap().deref_mut());
        channel.send_message(
            format!("Will check for {}", item).to_string(),
            &telegram_api,
        );

        let response = check_food(item.to_string());

        channel.send_message(format!("{}", response), &telegram_api);
    } else if content == "/help" {
        channel.send_message(
            "/menu [food name] => tells you where that food is being served today".to_string(),
            &telegram_api,
        );

        channel.send_message(
                    "/register [food name] => schedules it to tell you each day where that food is being served that day"
                        .to_string(),
                    &telegram_api,
                );
    } else if content.starts_with("/room ") {
        let room: String = content[6..].to_string();

        let res = send_get(format!("http://localhost:8000/room/?room={}", room));

        channel.send_message(format!("{}", res), &telegram_api);
    } else if content == "/run" {
        channel.send_message(
            "Checking for preregistered foods".to_string(),
            &telegram_api,
        );
        check_for_foods(&listeners, &telegram_api);
    } else if content.starts_with("/quit") && author.is_owner {
        channel.send_message("UMass Bot Quitting".to_string(), &telegram_api);
        std::process::exit(0);
    }
}

fn read_listeners() -> Vec<(TelegramChannel, String)> {
    let mut listeners_string: String = String::new();
    let _ = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .open("telegram_listeners.txt")
        .expect("No telegram listeners file")
        .read_to_string(&mut listeners_string);

    let mut listeners: Vec<(TelegramChannel, String)> = vec![];

    for line in listeners_string.split('\n') {
        if line == "" {
            continue;
        }
        let sections: Vec<&str> = line.split(' ').collect();
        let app = sections[0];
        if app == "telegram" {
            let id = TelegramChannel::ChannelId(telegram_bot::types::ChannelId::from(
                sections[1]
                    .parse::<i64>()
                    .expect("Couldn\'t parse channel id"),
            ));

            let food: String = sections[2..].join(" ").to_string();
            listeners.push((id, food));
        }
    }

    listeners
}

fn save_listeners(pairs: &[(TelegramChannel, String)]) {
    let mut listeners_string: String = String::new();
    pairs.iter().for_each(|x| {
        listeners_string = match *x {
            (ref id, ref food) => format!("{}telegram {:?} {}\n", listeners_string, id, food),
        };
    });

    let listeners_string = listeners_string.trim();

    let _ = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(true)
        .open("telegram_listeners.txt")
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

fn check_for_foods(listeners: &Arc<Mutex<Vec<(TelegramChannel, String)>>>, telegram_api: &Api) {
    listeners
        .lock()
        .unwrap()
        .to_vec()
        .into_iter()
        .for_each(|(channel, food)| {
            println!("Checking on {:?} for {}", channel, food);
            let response = check_food(food);

            channel.send_message(format!("{}", response), telegram_api);
        });
}

fn main() {
    // Allow openssl crosscompiling to work
    openssl_probe::init_ssl_cert_env_vars();

    let listeners: Arc<Mutex<Vec<(TelegramChannel, String)>>> =
        Arc::new(Mutex::new(read_listeners()));

    let telegram_token = load_telegram_token();

    // Listeners loop
    /*let listeners_clone = Arc::clone(&listeners);
    let telegram_token_clone = telegram_token.clone();
    thread::spawn(move || {
        let listeners = listeners_clone;
        loop {
            let telegram_token = telegram_token_clone.clone();
            println!("Seconds till scheduled: {:?}", get_time_till_scheduled());
            thread::sleep(get_time_till_scheduled());
            println!("Checking for foods now!");
            check_for_foods(&listeners, &telegram_token);
        }
    });*/

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
                            TelegramChannel::ChatMessage(message.chat),
                            Arc::clone(&listeners),
                            &api,
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
