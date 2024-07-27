#![allow(unused)]

#[macro_use]
extern crate lazy_static;

use anyhow::{anyhow, Result};
mod core;
mod envelope;
mod executive;
mod house;
mod mailing;
mod military;
mod models;
mod nasa;
mod observer;
mod prsr;
mod senate;
mod state;
mod usps;
mod postage_statement;
use core::*;
use executive::*;
use house::*;
use mailing::*;
use military::*;
use models::*;
use nasa::*;
use observer::*;
use prsr::*;
use senate::*;
use state::*;
use usps::*;

#[tokio::main]
pub async fn main() -> Result<()> {
    // Load addresses from disk or network.
    let mut military = Military::load().await?;
    let mut nasa = Nasa::load().await?;
    let mut executive = Executive::load().await?;
    let mut senate = Senate::load().await?;
    let mut house = House::load().await?;
    let mut state = State::load().await?;
    let mut observer = Observer::load().await?;

    // Combine people into single list.
    let mut pers = Vec::with_capacity(1_076);
    pers.extend(military.persons);
    pers.extend(nasa.persons);
    pers.extend(executive.persons);
    pers.extend(senate.persons);
    pers.extend(house.persons);
    pers.extend(state.persons);
    pers.extend(observer.persons);
    eprintln!("{} people", pers.len());

    // Create mailing.
    let mut mailing = Mailing::load(&mut pers).await?;

    Ok(())
}
