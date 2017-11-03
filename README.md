# UMass Events Bot

## Usage

The program requires a bot token to connect to Discord. It does this using a "token" file that is not checked in to git, so that the bot cannot be messed with.

To add it to your server, go to [this link](https://discordapp.com/api/oauth2/authorize?client_id=355392985912836097&scope=bot&permissions=1) and authorize the bot.

## Crosscompiling for Linux

Using: https://github.com/emk/rust-musl-builder

Set the alias for the builder:

``alias rust-musl-builder='docker run --rm -it -v "$(pwd)":/home/rust/src ekidd/rust-musl-builder'``

Start the Docker daemon (if it isn't already running)

Build:

``rust-musl-builder cargo build --release``

The built file will be in ``target/x86_64-unknown-linux-musl/release``

## Commands

| Command               | Description                                                                 |
| --------------------- | --------------------------------------------------------------------------- |
| !menu [food name]     | tells you where that food is being served today                             |
| !register [food name] | schedules it to tell you each day where that food is being served that day  |
| !events               | tells you the events for that day [[Currently broken!]]                     |

## TODO

* Add !help
* Add !deregister
* Fix !events
* Allow authorized users (ie. gbear605) to make announcements to all servers the bot is on
* Make !menu and scheduled food announcements say what the entire name of that food is (so searching for pizza would say all the different types of pizzas at each DC)