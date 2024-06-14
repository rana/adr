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
            // Select first name and last name.
            // Full name may have middle name or suffix.
            let names = full_name
                .split_whitespace()
                .filter(|&w| w != "Gov." && w != "Jr." && w != "III")
                .map(|w| w.trim_end_matches(','))
                .collect::<Vec<_>>();
            per.name_fst = names[0].trim().to_string();
            per.name_lst = names[names.len() - 1].trim().to_string();

            // Validate fields.
            if per.name_fst.is_empty() {
                return Err(anyhow!("first name empty{:?}", per));
            }
            if per.name_lst.is_empty() {
                return Err(anyhow!("last name empty {:?}", per));
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
        for (idx, state) in state_names().iter().enumerate().take(1) {
            let url = format!("https://www.nga.org/governors/{state}/");
            let html = fetch_html(&url).await?;

            match prs_adr_lnes(&html) {
                None => return Err(anyhow!("no lines for {url}")),
                Some(adr_lnes) => match PRSR.prs_adrs(&adr_lnes) {
                    None => return Err(anyhow!("no address for {url}")),
                    Some(mut adrs) => {
                        adrs = standardize_addresses(adrs).await?;
                        self.persons[idx].adrs = Some(adrs);
                    }
                },
            }

            // Checkpoint save.
            // Write intermediate file to disk.
            write_to_file(&self, FLE_PTH)?;
        }

        Ok(())
    }
}

pub fn prs_adr_lnes(html: &str) -> Option<Vec<String>> {
    let document = Html::parse_document(html);

    // Attempt to select addresses from various sections of the HTML.
    let mut lnes: Vec<String> = Vec::new();
    for txt in ["li.item", "body"] {
        let selector = Selector::parse(txt).unwrap();
        for elm in document.select(&selector) {
            // Extract lines from html.
            let mut cur_lnes = elm
                .text()
                .map(|s| s.trim().trim_end_matches(',').to_uppercase().to_string())
                .collect::<Vec<String>>();

            // Filter lines.
            // Filter separately to allow debugging.
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
    edit_nbsp(&mut lnes);
    // edit_person_senate_lnes(per, &mut lnes);
    PRSR.edit_lnes(&mut lnes);
    edit_newline(&mut lnes);
    // edit_sob(&mut lnes);
    edit_split_comma(&mut lnes);
    edit_mailing(&mut lnes);
    edit_starting_hash(&mut lnes);
    edit_char_half(&mut lnes);
    edit_empty(&mut lnes);

    eprintln!("--- post: {lnes:?}");

    // At least one office in home state, and one in DC.
    if PRSR.two_zip_or_more(&lnes) {
        return Some(lnes);
    }

    None
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
