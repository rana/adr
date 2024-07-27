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

const FLE_PTH: &str = "state.json";

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct State {
    pub name: String,
    pub role: Role,
    pub persons: Vec<Person>,
}

impl State {
    pub fn new() -> Self {
        // In the United States, there are a total of 55 governors. This includes: 50 state governors (one for each of the 50 states). 5 territorial governors for the following U.S. territories: American Samoa, Guam, Northern Mariana Islands, Puerto Rico, U.S. Virgin Islands.
        Self {
            name: "U.S. Governors".into(),
            role: Role::Political,
            persons: Vec::with_capacity(55),
        }
    }

    pub async fn load() -> Result<State> {
        // Read file from disk.
        let mut state = match read_from_file::<State>(FLE_PTH) {
            Ok(state_from_disk) => state_from_disk,
            Err(_) => {
                let mut state = State::new();

                // Fetch members.
                for state_name in state_names() {
                    let per = state.fetch_member(state_name).await?;
                    state.persons.push(per);
                }

                // Write file to disk.
                write_to_file(&state, FLE_PTH)?;

                state
            }
        };

        println!("{} governors", state.persons.len());

        // Fetch addresses.
        state.fetch_adrs().await?;

        Ok(state)
    }

    /// Fetch member from network.
    pub async fn fetch_member(&self, state_name: &str) -> Result<Person> {
        let url = format!("https://www.nga.org/governors/{state_name}/");
        let html = fetch_html(&url).await?;
        let document = Html::parse_document(&html);
        let mut per = Person::default();

        // Select name.
        let name_sel = Selector::parse("h1.title").expect("Invalid selector");
        if let Some(elm) = document.select(&name_sel).next() {
            let full_name = elm.text().collect::<Vec<_>>().concat();
            per.name = name_clean(&full_name);
            if per.name.is_empty() {
                return Err(anyhow!("name is empty{:?}", per));
            }
        }

        // Select url.
        // May not exist.
        let url_sel = Selector::parse("li.item").expect("Invalid selector");
        let link_sel = Selector::parse("a").expect("Invalid selector");
        for doc_elm in document.select(&url_sel) {
            if let Some(elm_url) = doc_elm.select(&link_sel).next() {
                if elm_url.inner_html().to_uppercase() == "GOVERNOR'S WEBSITE" {
                    per.url = elm_url
                        .value()
                        .attr("href")
                        .unwrap_or_default()
                        .trim_end_matches('/')
                        .to_string();
                }
            }
        }

        Ok(per)
    }

    pub async fn fetch_adrs(&mut self) -> Result<()> {
        // Clone self for file writing.
        let mut self_clone = self.clone();
        let per_len = self.persons.len() as f64;
        let state_names = state_names();

        for (idx, per) in self_clone
            .persons
            .iter()
            .enumerate()
            .filter(|(_, per)| per.adrs.is_none())
        // .take(1)
        {
            let mut state = state_names[idx];
            if state == "virgin-islands" {
                state = "u-s-virgin-islands";
            }
            let mut url = format!("https://www.usa.gov/states/{}", state);
            if state == "guam" {
                url.clone_from(&per.url);
            }

            let pct = (((idx as f64 + 1.0) / per_len) * 100.0) as u8;
            eprintln!("  {}% {} {} {}", pct, idx, state, url);

            if state == "new-york" {
                let adr = Address {
                    address1: "NYS STATE CAPITOL BUILDING".into(),
                    city: "ALBANY".into(),
                    state: "NY".into(),
                    zip5: 12224,
                    delivery_point: None,
                    ..Default::default()
                };
                self.persons[idx].adrs = Some(vec![adr]);
            } else if state == "american-samoa" {
                let adr = Address {
                    address1: "OFFICE OF THE GOVERNOR".into(),
                    city: "PAGO PAGO".into(),
                    state: "AS".into(),
                    zip5: 96799,
                    delivery_point: None,
                    ..Default::default()
                };
                self.persons[idx].adrs = Some(vec![adr]);
            } else {
                // Fetch, parse, standardize.
                match fetch_prs_std_adrs(state, &url).await? {
                    None => {}
                    Some(mut adrs) => {
                        self.persons[idx].adrs = Some(adrs);
                    }
                }
            }

            // Checkpoint save.
            // Write intermediate file to disk.
            write_to_file(&self, FLE_PTH)?;
        }

        Ok(())
    }
}

/// Fetch and parse addresses and standardize with the USPS.
pub async fn fetch_prs_std_adrs(state: &str, url: &str) -> Result<Option<Vec<Address>>> {
    // Fetch html.
    let html = fetch_html(url).await?;

    // Parse html to address lines.
    let adr_lnes_o = prs_adr_lnes(state, &html);

    // Parse lines to addresses.
    let adrs_o = match adr_lnes_o {
        None => None,
        Some(mut adr_lnes) => match PRSR.prs_adrs(&adr_lnes) {
            None => None,
            Some(mut adrs) => {
                adrs = standardize_addresses(adrs).await?;
                if adrs.is_empty() {
                    None
                } else {
                    Some(adrs)
                }
            }
        },
    };

    Ok(adrs_o)
}

pub fn prs_adr_lnes(state: &str, html: &str) -> Option<Vec<String>> {
    let document = Html::parse_document(html);
    let mut lnes: Vec<String> = Vec::new();
    for txt in ["span.field", "li.item", "body"] {
        let selector = Selector::parse(txt).unwrap();
        for elm in document.select(&selector) {
            // Extract lines from html.
            let mut cur_lnes = elm
                .text()
                .map(|s| s.trim().trim_end_matches(',').to_uppercase().to_string())
                .collect::<Vec<String>>();

            // eprintln!("--- pre: {cur_lnes:?}");

            // Filter lines.
            cur_lnes = cur_lnes
                .into_iter()
                .filter(|s| PRSR.filter(s))
                .collect::<Vec<String>>();

            eprintln!("{cur_lnes:?}");

            lnes.extend(cur_lnes);
        }

        if !lnes.is_empty() {
            break;
        }
    }

    // eprintln!("--- pre: {lnes:?}");

    // Edit lines to make it easier to parse.
    edit_dot(&mut lnes);
    edit_nbsp_zwsp(&mut lnes);
    edit_mailing(&mut lnes);
    edit_person_state_lnes(state, &mut lnes);
    PRSR.edit_lnes(&mut lnes);
    edit_newline(&mut lnes);
    edit_split_comma(&mut lnes);
    edit_starting_hash(&mut lnes);
    edit_char_half(&mut lnes);
    edit_empty(&mut lnes);

    eprintln!("--- --- --- post: {lnes:?}");

    // Do not check for zip count here.

    Some(lnes)
}

pub fn edit_person_state_lnes(state: &str, lnes: &mut [String]) {
    match state {
        "indiana" => {
            for idx in (0..lnes.len()).rev() {
                if lnes[idx] == "STATEHOUSE" {
                    lnes[idx] = "200 W WASHINGTON ST STE 206".into();
                }
            }
        }
        "new-jersey" => {
            for idx in (0..lnes.len()).rev() {
                if lnes[idx].contains("PO BOX") {
                    lnes[idx] = lnes[idx].replace("PO BOX", ",PO BOX");
                }
            }
        }
        "georgia" => {
            for idx in (0..lnes.len()).rev() {
                if lnes[idx] == "SUITE 203, STATE CAPITOL" {
                    lnes[idx] = "STE 203".into();
                }
            }
        }
        "massachusetts" => {
            for idx in (0..lnes.len()).rev() {
                if lnes[idx] == "OFFICE OF THE GOVERNOR, ROOM 280" {
                    lnes[idx] = "ROOM 280".into();
                }
            }
        }
        "northern-mariana-islands" => {
            for idx in (0..lnes.len()).rev() {
                if lnes[idx].contains("CALLER BOX") {
                    lnes[idx] = lnes[idx].replace("CALLER BOX", "PO BOX");
                }
            }
        }
        "u-s-virgin-islands" => {
            for idx in (0..lnes.len()).rev() {
                if lnes[idx].contains("(21-22)") {
                    lnes[idx] = lnes[idx].replace("(21-22)", "");
                }
            }
        }
        "" => {}
        _ => {}
    }
}

fn state_names() -> Vec<&'static str> {
    vec![
        "alabama",
        "alaska",
        "arizona",
        "arkansas",
        "california",
        "colorado",
        "connecticut",
        "delaware",
        "florida",
        "georgia",
        "hawaii",
        "idaho",
        "illinois",
        "indiana",
        "iowa",
        "kansas",
        "kentucky",
        "louisiana",
        "maine",
        "maryland",
        "massachusetts",
        "michigan",
        "minnesota",
        "mississippi",
        "missouri",
        "montana",
        "nebraska",
        "nevada",
        "new-hampshire",
        "new-jersey",
        "new-mexico",
        "new-york",
        "north-carolina",
        "north-dakota",
        "ohio",
        "oklahoma",
        "oregon",
        "pennsylvania",
        "rhode-island",
        "south-carolina",
        "south-dakota",
        "tennessee",
        "texas",
        "utah",
        "vermont",
        "virginia",
        "washington",
        "west-virginia",
        "wisconsin",
        "wyoming",
        "american-samoa",
        "guam",
        "northern-mariana-islands",
        "puerto-rico",
        "virgin-islands",
    ]
}
