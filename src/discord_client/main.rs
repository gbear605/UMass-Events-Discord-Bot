#![feature(vec_remove_item)]

extern crate chrono;
extern crate futures;
extern crate openssl_probe;
extern crate reqwest;
extern crate select;
extern crate serenity;
extern crate tokio;
extern crate tokio_core;
extern crate umass_bot_common;

use serenity::framework::standard::help_commands;
use serenity::framework::standard::macros::help;
use serenity::framework::standard::Args;
use serenity::framework::standard::CommandGroup;
use serenity::framework::standard::HelpOptions;
use serenity::model::event::ResumedEvent;
use serenity::model::gateway::Ready;
use serenity::model::id::UserId;
use umass_bot_common::datetime::get_time_till_scheduled;
use umass_bot_common::error::*;

// For discord
use serenity::client::Client;
use serenity::http::GuildPagination;
use serenity::http::Http;

use serenity::prelude::*;

use serenity::{
    framework::standard::{
        macros::{command, group},
        CommandResult, StandardFramework,
    },
    model::{
        channel::Message,
        id::{ChannelId, GuildId},
    },
};

use std::collections::HashSet;

// For file reading
use std::fs::File;
use std::fs::OpenOptions;

use std::io::Read;
use std::io::Write;

// For multithreading
use std::sync::Arc;
use std::thread;

struct Listeners {}

impl TypeMapKey for Listeners {
    type Value = Vec<(ChannelId, String)>;
}

// Checks that a message successfully sent; if not, then logs why to stdout.
fn check_msg<T>(result: serenity::Result<T>) {
    if let Err(why) = result {
        println!("Discord error: {:?}", why);
    }
}

fn send_message(channel_id: ChannelId, message: &str, http: &Arc<Http>) {
    check_msg(channel_id.say(http, message));
}

fn check_food(food: &str) -> Result<String> {
    let client = reqwest::Client::new();
    Ok(client
        .get("http://localhost:8000/food/")
        .query(&[("food", food)])
        .send()?
        .text()?)
}

// Get the discord token file from memory
fn load_discord_token() -> String {
    let mut token = String::new();
    let _ = File::open("discord_token")
        .expect("No token file")
        .read_to_string(&mut token);
    token
}

fn get_guilds(http: &Arc<Http>) -> Result<Vec<String>> {
    let guilds = http.get_guilds(&GuildPagination::After(GuildId(0)), 100)?;
    Ok(guilds.into_iter().map(|guild| guild.name).collect())
}

struct Handler {}

impl EventHandler for Handler {
    fn ready(&self, ctx: Context, ready: Ready) {
        println!("Connected to Discord as {}", ready.user.name);
        match get_guilds(&ctx.http).map(|guilds| guilds.join(", ")) {
            Ok(guilds) => println!("Connected to servers: {}", guilds),
            Err(err) => println!("Couldn't get guilds: {}", err),
        }
    }

    fn resume(&self, _: Context, _: ResumedEvent) {
        println!("Resumed");
    }
}

#[group]
#[commands(menu, echo, register, deregister, room, run)]
struct General;

#[group]
#[commands(quit)]
#[owners_only]
struct Admin;

#[command]
fn menu(ctx: &mut Context, msg: &Message, args: Args) -> CommandResult {
    let food: &str = args.rest();

    msg.reply(ctx, &check_food(food)?)?;
    Ok(())
}

#[command]
fn echo(ctx: &mut Context, msg: &Message, args: Args) -> CommandResult {
    let input: &str = args.rest();

    let client = reqwest::Client::new();
    let response = client
        .get("http://localhost:8000/echo/")
        .query(&[("input", input)])
        .send()?
        .text()?;

    msg.reply(ctx, &response)?;
    Ok(())
}

#[command]
fn register(ctx: &mut Context, msg: &Message, args: Args) -> CommandResult {
    let mut writable_data = ctx.data.write();
    let listeners = writable_data.get_mut::<Listeners>().unwrap();

    let food: &str = args.rest();

    listeners.push((msg.channel_id, food.to_string()));
    save_listeners(listeners)?;
    send_message(
        msg.channel_id,
        &format!("Will check for {}", food),
        &ctx.http,
    );

    send_message(msg.channel_id, &check_food(food)?, &ctx.http);

    Ok(())
}

#[command]
fn deregister(ctx: &mut Context, msg: &Message, args: Args) -> CommandResult {
    let mut writable_data = ctx.data.write();
    let listeners = writable_data.get_mut::<Listeners>().unwrap();

    let food: &str = args.rest();

    let to_remove = (msg.channel_id, food.to_string());
    if listeners.contains(&to_remove) {
        listeners.remove_item(&to_remove);
        save_listeners(listeners)?;
        send_message(msg.channel_id, &format!("Removed {}", food), &ctx.http);
    } else {
        send_message(
            msg.channel_id,
            &format!("Couldn't find {}", food),
            &ctx.http,
        );
    }

    Ok(())
}

#[command]
fn room(ctx: &mut Context, msg: &Message, args: Args) -> CommandResult {
    let room: &str = args.rest();

    let client = reqwest::Client::new();

    let res = client
        .get("http://localhost:8000/room/")
        .query(&[("room", room)])
        .send()?;

    let response = if res.status().is_success() {
        "Some classes meet in that room"
    } else {
        "No classes meet in that room"
    };

    msg.reply(ctx, response)?;
    Ok(())
}

#[command]
fn run(ctx: &mut Context, msg: &Message) -> CommandResult {
    let mut writable_data = ctx.data.write();
    let listeners = writable_data.get_mut::<Listeners>().unwrap();

    let http = Arc::clone(&ctx.http);
    let listeners_clone = listeners.clone();
    thread::spawn(move || {
        println!("Checking for foods now!");
        check_for_foods(listeners_clone.to_vec(), &http);
    });

    send_message(
        msg.channel_id,
        "Checking for preregistered foods",
        &ctx.http,
    );

    Ok(())
}

#[command]
fn quit(ctx: &mut Context, msg: &Message) -> CommandResult {
    send_message(msg.channel_id, "UMass Bot Quitting", &ctx.http);
    std::process::exit(0);
}

#[help]
fn default_help_command(
    context: &mut Context,
    msg: &Message,
    args: Args,
    help_options: &'static HelpOptions,
    groups: &[&'static CommandGroup],
    owners: HashSet<UserId>,
) -> CommandResult {
    help_commands::plain(context, msg, args, help_options, groups, owners)
}

fn read_listeners() -> Vec<(ChannelId, String)> {
    umass_bot_common::listeners::read_listeners_generic("discord_listeners.txt", &|s: String| {
        ChannelId(s.parse::<u64>().expect("Couldn't parse channel id"))
    })
}

fn save_listeners(pairs: &[(ChannelId, String)]) -> Result<()> {
    let mut listeners_string: String = String::new();
    pairs.iter().for_each(|x| {
        listeners_string = match *x {
            (ref id, ref food) => format!("{}discord {} {}\n", listeners_string, id, food),
        };
    });

    let listeners_string = listeners_string.trim();

    OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(true)
        .open("discord_listeners.txt")?
        .write_all(listeners_string.as_bytes())?;

    Ok(())
}

fn check_for_foods(listeners: Vec<(ChannelId, String)>, http: &Arc<Http>) {
    listeners.into_iter().for_each(|(channel, food)| {
        println!("Checking on {:?} for {}", channel, food);
        match check_food(&food) {
            Ok(response) => send_message(channel, &response, http),
            Err(_) => send_message(channel, &format!("Couldn't check for {}", food), http),
        }
    });
}

fn main() {
    // Allow openssl crosscompiling to work
    openssl_probe::init_ssl_cert_env_vars();

    // Setup discord
    let mut client =
        Client::new(load_discord_token().trim(), Handler {}).expect("Error creating client");
    client.data.write().insert::<Listeners>(read_listeners());

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
    let data_clone = Arc::clone(&client.data);
    let http = Arc::clone(&client.cache_and_http.http);
    thread::spawn(move || {
        let data = data_clone;
        loop {
            println!("Seconds till scheduled: {:?}", get_time_till_scheduled());
            thread::sleep(get_time_till_scheduled());
            println!("Checking for foods now!");
            check_for_foods(data.write().get::<Listeners>().unwrap().to_vec(), &http);
        }
    });

    client.with_framework(
        StandardFramework::new()
            .configure(|c| c.prefix("!").owners(owners))
            .group(&GENERAL_GROUP)
            .group(&ADMIN_GROUP)
            .help(&DEFAULT_HELP_COMMAND),
    );

    if let Err(why) = client.start() {
        println!("Discord client error: {:?}", why);
    }
}
