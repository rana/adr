#![allow(unused)]

#[macro_use]
extern crate lazy_static;

use anyhow::{anyhow, Result};
mod core;
mod executive;
mod house;
mod mailing;
mod military;
mod models;
mod nasa;
mod prsr;
mod senate;
mod state;
mod usps;
use core::*;
use executive::*;
use house::*;
use mailing::*;
use military::*;
use models::*;
use nasa::*;
use prsr::*;
use senate::*;
use state::*;
use usps::*;

#[tokio::main]
pub async fn main() -> Result<()> {
    let mut military = Military::load().await?;
    let mut nasa = Nasa::load().await?;
    let mut executive = Executive::load().await?;
    let mut senate = Senate::load().await?;
    let mut house = House::load().await?;
    let mut state = State::load().await?;

    let mut pers = Vec::with_capacity(2_000);
    pers.extend(military.persons);
    pers.extend(nasa.persons);
    pers.extend(executive.persons);
    pers.extend(senate.persons);
    pers.extend(house.persons);
    pers.extend(state.persons);
    eprintln!("{} people", pers.len());

    let mut adrs = Vec::with_capacity(4_000);
    for per in pers.iter() {
        if per.adrs.is_none() {
            return Err(anyhow!("{} has no addresses", per.name));
        }
        if let Some(per_adrs) = &per.adrs {
            adrs.extend(per_adrs);
        }
    }
    eprintln!("{} addresses", adrs.len());

    Ok(())
}
