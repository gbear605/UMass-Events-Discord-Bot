# UMass Events Bot

[![builds.sr.ht status](https://builds.sr.ht/~gbear605/umass_discord_bot.svg)](https://builds.sr.ht/~gbear605/umass_discord_bot?)

## Usage

The program requires a bot token to connect to Discord. It does this using a "token" file that is not checked in to git, so that the bot cannot be messed with.

To add it to your server, go to [this link](https://discordapp.com/api/oauth2/authorize?client_id=355392985912836097&scope=bot&permissions=1) and authorize the bot.

## Crosscompiling for Linux

Using: https://github.com/emk/rust-musl-builder

Docker needs to be installed.

1) Set the alias for the builder:

``alias rust-musl-builder='docker run --rm -it -v cargo-git:/home/rust/.cargo/git -v cargo-registry:/home/rust/.cargo/registry -v "$(pwd)":/home/rust/src ekidd/rust-musl-builder:nightly-2019-06-08-openssl11'``

2) Start the Docker daemon (if it isn't already running)

3) Set up the file cache: (speeds up build time)

``rust-musl-builder sudo chown -R rust:rust /home/rust/.cargo/git /home/rust/.cargo/registry``

4) Build: (this will download a linux docker image to compile in if it is not already downloaded)

Expect the build step to initially take about fifteen minutes, then to take a while (roughly a minute) each time after the first.

``rust-musl-builder cargo build --bin telegram_client --release``
``rust-musl-builder cargo build --bin discord_client --release``
``rust-musl-builder cargo build --bin server --release``

To update the container, run ``docker pull ekidd/rust-musl-builder:nightly-2019-06-08-openssl11``

The built files will be in ``target/x86_64-unknown-linux-musl/release``

## Commands

| Command               | Description                                                                 |
| --------------------- | --------------------------------------------------------------------------- |
| !menu [food name]     | tells you where that food is being served today                             |
| !register [food name] | schedules it to tell you each day where that food is being served that day  |
| !events               | tells you the events for that day [[Currently broken!]]                     |
| !help                 | tells you this list of commands                                             |

## TODO

* Add !deregister
* Fix !events
* Add !time that says how long until the next scheduled food announcement
* Allow authorized users (ie. gbear605) to make announcements to all servers the bot is on
* Allow manual input of food for future days, for late night menus.
