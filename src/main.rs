#![allow(unused)]

#[macro_use]
extern crate lazy_static;

use anyhow::{anyhow, Result};
mod core;
mod state;
mod house;
mod mailing;
mod military;
mod models;
mod prsr;
mod senate;
mod usps;
use core::*;
use state::*;
use house::*;
use mailing::*;
use military::*;
use models::*;
use prsr::*;
use senate::*;
use usps::*;

#[tokio::main]
pub async fn main() -> Result<()> {
    // let mut military = Military::load().await?;

    let mut house = House::load().await?;

    // let mut senate = Senate::load().await?;
    
    // let mut state = State::load().await?;

    // TODO: SCIENTIFC LEADERS

    // TODO: EXECUTIVE BRANCH

    Ok(())
}
