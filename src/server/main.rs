#![feature(proc_macro_hygiene, decl_macro)]

#[macro_use]
extern crate rocket;

extern crate rocket_contrib;
extern crate umass_bot_common;

use umass_bot_common::error::*;

mod events;
mod food;
mod rooms;

use crate::rooms::Section;
use food::FoodStore;

use rocket::State;
use rooms::load_sections_map;
use rooms::RoomStore;

use rocket_contrib::json::Json;

#[get("/?<input>")]
fn echo(input: String) -> String {
    input
}

#[get("/?<room>")]
fn room(room_store: State<RoomStore>, room: String) -> Option<Json<Vec<Section>>> {
    if !room_store.contains_key(&room) {
        None
    } else {
        Some(Json(room_store.get(&room).unwrap().to_vec()))
    }
}

#[get("/?<food>")]
fn food(food_store: State<FoodStore>, food: String) -> Result<String> {
    let places_found = food::get_food_on_menus(&food, &food_store)?;

    Ok(match places_found.len() {
        0 => format!("{} not found", food).to_string(),
        _ => format!("{}: \n{}", food, places_found.join("\n")).to_string(),
    })
}

fn main() {
    rocket::ignite()
        .manage(load_sections_map())
        .manage(food::get_store())
        .mount("/echo", routes![echo])
        .mount("/room", routes![room])
        .mount("/food", routes![food])
        .launch();
}
