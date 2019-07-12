# UMass Events Bot

[![builds.sr.ht status](https://builds.sr.ht/~gbear605/umass_discord_bot.svg)](https://builds.sr.ht/~gbear605/umass_discord_bot?)

A Discord and Telegram bot written in Rust to enable easy access to a variety of University of Massachusetts, Amherst services. Currently, it provides the ability to search across the dining halls to check which foods are present on a given day, as well as information on room availability to ease studying.

## Usage

To add it to your server, go to [this link](https://discordapp.com/api/oauth2/authorize?client_id=355392985912836097&scope=bot&permissions=1) and authorize the bot.

## Development

The program requires a bot token to connect to Discord or Telegram. It does this using private token files (`discord_token` and `telegram_token` respectively) which can be acquired from Discord and Telegram.

## Crosscompiling for Linux

Using: https://github.com/emk/rust-musl-builder

Docker needs to be installed.

1) Set the alias for the builder:

``alias rust-musl-builder='docker run --rm -it -v cargo-git:/home/rust/.cargo/git -v cargo-registry:/home/rust/.cargo/registry -v "$(pwd)":/home/rust/src ekidd/rust-musl-builder:nightly-2019-06-08'``

2) Start the Docker daemon (if it isn't already running)

3) Set up the file cache: (speeds up build time)

``rust-musl-builder sudo chown -R rust:rust /home/rust/.cargo/git /home/rust/.cargo/registry``

4) Build: (this will download a linux docker image to compile in if it is not already downloaded)

Expect the build step to initially take about fifteen minutes, then to take a while (roughly a minute) each time after the first.

``rust-musl-builder cargo build --bin telegram_client --release``
``rust-musl-builder cargo build --bin discord_client --release``
``rust-musl-builder cargo build --bin server --release``

To update the container, run ``docker pull ekidd/rust-musl-builder:nightly-2019-06-08``

The built files will be in ``target/x86_64-unknown-linux-musl/release``

## Commands

| Command               | Description                                                                 |
| --------------------- | --------------------------------------------------------------------------- |
| !menu [food name]     | tells you where that food is being served today                             |
| !register [food name] | schedules it to tell you each day where that food is being served that day  |
| !room [room name]     | checks whether the given room is currently in use by a class or free        |
| !help                 | tells you this list of commands                                             |

## TODO

* Add !deregister
* Add !events
* Add !time that says how long until the next scheduled food announcement
* Allow authorized users to make announcements to all servers the bot is on
* Allow manual input of food for future days, for late night menus.
* Add more usage of classroom data
