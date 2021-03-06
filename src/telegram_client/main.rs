#![feature(vec_remove_item)]

extern crate chrono;
extern crate futures;
extern crate openssl_probe;
extern crate select;
extern crate telegram_bot;
extern crate tokio;
extern crate tokio_core;

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

// For requests to server
use reqwest::Url;

use tokio_core::reactor::Core;

#[derive(Debug, Clone, PartialEq)]
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
    fn send_message(&self, message: &str, api: &Api) {
        let send_message = self.to_chat_ref().text(message);
        api.spawn(send_message);
    }
}

type ResponseCode = reqwest::StatusCode;

fn send_get(url: String) -> (String, ResponseCode) {
    let url = url.replace(" ", "%20");

    let mutex = Arc::new(Mutex::new(None));
    let mutex_c = mutex.clone();

    let t = thread::spawn(move || {
        let client = reqwest::Client::new();
        let mut response = client
            .get::<Url>(url.parse::<Url>().unwrap())
            .send()
            .unwrap();
        *mutex_c.lock().unwrap() = Some((response.text().unwrap(), response.status()));
    });

    t.join().unwrap();
    let unlocked_mutex = mutex.lock().unwrap();
    let (body, status) = unlocked_mutex.as_ref().unwrap();
    (body.to_string(), *status)
}

fn check_food(food: String) -> String {
    send_get(format!("http://localhost:8000/food?food={}", food)).0
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

        let response = check_food(item.to_string());

        channel.send_message(&response, &telegram_api);
    } else if content.starts_with("/echo ") {
        let input: String = content[6..].to_string();

        let res = send_get(format!("http://localhost:8000/echo?input={}", input)).0;

        channel.send_message(&res, &telegram_api);
    } else if content.starts_with("/register ") {
        let item: String = content[10..].to_string();
        listeners
            .lock()
            .unwrap()
            .deref_mut()
            .push((channel.clone(), item.clone()));
        save_listeners(listeners.lock().unwrap().deref_mut());
        channel.send_message(&format!("Will check for {}", item), &telegram_api);

        let response = check_food(item.to_string());

        channel.send_message(&response, &telegram_api);
    } else if content.starts_with("/deregister ") {
        let item: &str = &content[12..];

        let to_remove = (channel.clone(), item.to_string());

        let mut unlocked_listeners = listeners.lock().unwrap();
        let listeners = unlocked_listeners.deref_mut();

        if listeners.contains(&to_remove) {
            listeners.remove_item(&to_remove);
            save_listeners(listeners);
            channel.send_message(&format!("Removed {}", item), &telegram_api);
        } else {
            channel.send_message(&format!("Couldn't find {}", item), &telegram_api);
        }
    } else if content == "/help" {
        channel.send_message(
            "/menu [food name] => tells you where that food is being served today",
            &telegram_api,
        );

        channel.send_message("/register [food name] => schedules it to tell you each day where that food is being served that day", &telegram_api);

        channel.send_message(
            "/deregister [food name] => removes a registered food",
            &telegram_api,
        );
    } else if content.starts_with("/room ") {
        let room: String = content[6..].to_string();

        let (_, status_code) = send_get(format!("http://localhost:8000/room/?room={}", room));

        if status_code == 200 {
            channel.send_message("Rooms found", &telegram_api);
        } else {
            channel.send_message("No rooms found", &telegram_api);
        }
    } else if content == "/run" {
        channel.send_message("Checking for preregistered foods", &telegram_api);
        check_for_foods(&listeners, &telegram_api);
    } else if content.starts_with("/quit") && author.is_owner {
        channel.send_message("UMass Bot Quitting", &telegram_api);
        std::process::exit(0);
    }
}

fn read_listeners() -> Vec<(TelegramChannel, String)> {
    umass_bot_common::listeners::read_listeners_generic("telegram_listeners.txt", &|s: String| {
        TelegramChannel::ChannelId(telegram_bot::types::ChannelId::from(
            s.parse::<i64>().expect("Couldn\'t parse channel id"),
        ))
    })
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

fn check_for_foods(listeners: &Arc<Mutex<Vec<(TelegramChannel, String)>>>, telegram_api: &Api) {
    listeners
        .lock()
        .unwrap()
        .to_vec()
        .into_iter()
        .for_each(|(channel, food)| {
            println!("Checking on {:?} for {}", channel, food);
            let response = check_food(food);

            channel.send_message(&response, telegram_api);
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
            check_for_foods(&listeners, &api);
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
