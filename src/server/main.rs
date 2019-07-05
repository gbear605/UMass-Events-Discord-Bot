#![feature(proc_macro_hygiene, decl_macro)]

#[macro_use]
extern crate rocket;

mod rooms;

use rocket::State;
use rooms::load_sections_map;
use rooms::RoomStore;

use std::str;

#[post("/", data = "<input>")]
fn echo(input: String) -> String {
    input
}

#[get("/?<room>")]
fn room(room_store: State<RoomStore>, room: String) -> String {
    if !room_store.contains_key(&room) {
        format!("Room {} not found on SPIRE", room)
    } else {
        format!("Room {} found on SPIRE", room)
    }
}

fn main() {
    rocket::ignite()
        .manage(load_sections_map())
        .mount("/echo", routes![echo])
        .mount("/room", routes![room])
        .launch();
}
