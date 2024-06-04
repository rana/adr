#![allow(unused)]
use anyhow::{anyhow, Result};
mod core;
mod house;
mod mailing;
mod military;
mod models;
mod prsr;
mod usps;
use core::*;
use house::*;
use mailing::*;
use military::*;
use models::*;
use prsr::*;
use usps::*;

#[tokio::main]
pub async fn main() -> Result<()> {
    // let mut military = Military::load().await?;

    let mut house = House::load().await?;
    house.fetch_addresses().await?;

    Ok(())
}
