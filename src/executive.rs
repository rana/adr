use crate::core::*;
use crate::models::*;
use crate::prsr::*;
use crate::usps::*;
use anyhow::{anyhow, Result};
use reqwest::Client;
use scraper::{Html, Selector};
use serde::{Deserialize, Serialize};
use std::fs::File;
use std::io::{BufReader, BufWriter};
use std::ops::Add;
use std::path::Path;

const FLE_PTH: &str = "executive.json";
const FLE_PTH_URL: &str = "executive.url.json";

const CAP_PER: usize = 4;

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Executive {
    pub name: String,
    pub role: Role,
    pub persons: Vec<Person>,
}

impl Executive {
    pub fn new() -> Self {
        Self {
            name: "U.S. Executive Branch".into(),
            role: Role::Political,
            persons: Vec::new(),
        }
    }

    pub async fn load() -> Result<Executive> {
        // Read file from disk.
        let mut exec = match read_from_file::<Executive>(FLE_PTH) {
            Ok(exec_from_disk) => exec_from_disk,
            Err(err) => {
                let mut exec = Executive::new();

                // Set members.
                exec.persons = exec.set_members();

                // Write file to disk.
                write_to_file(&exec, FLE_PTH)?;

                exec
            }
        };

        println!("{} executive branch members", exec.persons.len());

        Ok(exec)
    }

    pub fn set_members(&self) -> Vec<Person> {
        let mut ret = Vec::new();

        // President
        let mut per = Person {
            name: "Joe Biden".into(),
            title1: "Office of the President".into(),
            url: "https://www.whitehouse.gov".into(),
            ..Default::default()
        };
        let adr = Address {
            address1: "1600 PENNSYLVANIA AVENUE NW".into(),
            city: "WASHINGTON".into(),
            state: "DC".into(),
            zip5: 20500,
            zip4: 5,
            delivery_point: Some("00".into()),
            ..Default::default()
        };
        per.adrs = Some(vec![adr]);
        ret.push(per);

        // Vice President
        let mut per = Person {
            name: "Kamala Harris".into(),
            title1: "Office of the Vice President".into(),
            url: "https://www.whitehouse.gov".into(),
            ..Default::default()
        };
        let adr = Address {
            address1: "EEOB".into(),
            city: "WASHINGTON".into(),
            state: "DC".into(),
            zip5: 20501,
            zip4: 1,
            delivery_point: Some("99".into()),
            ..Default::default()
        };
        per.adrs = Some(vec![adr]);
        ret.push(per);

        // Department of State
        let mut per = Person {
            name: "Antony Blinken".into(),
            title1: "Department of State".into(),
            url: "https://www.state.gov".into(),
            ..Default::default()
        };
        let adr = Address {
            address1: "2201 C STREET NW".into(),
            city: "WASHINGTON".into(),
            state: "DC".into(),
            zip5: 20520,
            zip4: 1,
            delivery_point: Some("01".into()),
            ..Default::default()
        };
        per.adrs = Some(vec![adr]);
        ret.push(per);

        ret
    }
}
