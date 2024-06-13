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

const FLE_PTH: &str = "house.json";

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct House {
    pub name: String,
    pub role: Role,
    pub persons: Vec<Person>,
}
impl House {
    pub fn new() -> Self {
        // The total number of members in the U.S. House of Representatives is 441. This includes 435 voting members who represent the 50 states and 6 non-voting members who represent the District of Columbia, Puerto Rico, and four other U.S. territories (American Samoa, Guam, the Northern Mariana Islands, and the U.S. Virgin Islands).
        // Some members may be vacant.
        Self {
            name: "U.S. House of Representatives".into(),
            role: Role::Political,
            persons: Vec::with_capacity(441),
        }
    }

    pub async fn load() -> Result<House> {
        // Read representatives file from disk.

        let house = match read_from_file::<House>(FLE_PTH) {
            Ok(house_from_disk) => {
                // Read from disk.
                house_from_disk
            }
            Err(err) => {
                // File not available.
                eprintln!("read file {}: err: {}", FLE_PTH, err);

                let mut house = House::new();

                // Fetch list of member from network.
                const URL: &str = "https://www.house.gov/representatives";

                // Fetch the members directory webpage.
                let cli = Client::new();
                let html = fetch_html(URL, &cli).await?;

                // Extract members from html.
                house.extract_members(&html);

                // Validate member fields.
                house.validate_members()?;

                // Write file to disk.
                write_to_file(&house, FLE_PTH)?;

                house
            }
        };

        println!("{} representatives", house.persons.len());

        Ok(house)
    }

    /// Extract members from the specified html.
    pub fn extract_members(&mut self, html: &str) {
        let document = Html::parse_document(html);

        // Define the CSS selector for the members list
        let selector = Selector::parse("table.table tr").unwrap();
        let name_selector = Selector::parse("td:nth-of-type(1)").unwrap();
        let url_selector = Selector::parse("td:nth-of-type(1) a").unwrap();

        // Iterate over each member entry
        for element in document.select(&selector) {
            if let Some(name_element) = element.select(&name_selector).next() {
                let mut per = Person::default();
                if let Some((name_lst, name_fst)) = name_element
                    .text()
                    .collect::<Vec<_>>()
                    .join(" ")
                    .split_once(',')
                {
                    per.name_fst = name_fst.trim().to_string();
                    per.name_lst = name_lst.trim().to_string();
                }
                // Skip empty or vacancy.
                // "Mike - Vacancy"
                if per.name_fst.is_empty() || per.name_fst.ends_with("Vacancy") {
                    continue;
                }
                per.url = element
                    .select(&url_selector)
                    .next()
                    .map_or(String::new(), |a| {
                        a.value()
                            .attr("href")
                            .unwrap_or_default()
                            .trim_end_matches('/')
                            .to_string()
                    });

                // Ensure url ends with ".house.gov".
                // https://katherineclark.house.gov/index.cfm/home"
                if !per.url.ends_with(".gov") {
                    if let Some(idx_fnd) = per.url.find(".gov") {
                        per.url.truncate(idx_fnd + 4);
                    }
                }

                self.persons.push(per);
            }
        }
    }

    pub fn validate_members(&self) -> Result<()> {
        for (idx, rep) in self.persons.iter().enumerate() {
            if rep.name_fst.is_empty() {
                return Err(anyhow!("house: first name empty (idx:{} {:?})", idx, rep));
            }
            if rep.name_lst.is_empty() {
                return Err(anyhow!("house: last name empty (idx:{} {:?})", idx, rep));
            }
            if rep.url.is_empty() {
                return Err(anyhow!("house: url empty (idx:{} {:?})", idx, rep));
            }
            if !rep.url.ends_with(".house.gov") {
                return Err(anyhow!(
                    "house: url doesn't end with '.house.gov' (idx:{} {:?})",
                    idx,
                    rep
                ));
            }
        }
        Ok(())
    }

    pub async fn fetch_addresses(&mut self) -> Result<()> {
        let cli = &Client::new();
        let prsr = &Prsr::new();

        // Clone self before iterating over self.persons to avoid borrowing conflicts.
        // For checkpoint saving.
        let mut self_clone = self.clone();
        let per_len = self.persons.len() as f64;
        for (idx, per) in self
            .persons
            .iter_mut()
            .enumerate()
            .filter(|(_, per)| per.adrs.is_none())
            .take(1)
        {
            let pct = (((idx as f64 + 1.0) / per_len) * 100.0) as u8;
            eprintln!(
                "  {}% {} {} {} {}",
                pct, idx, per.name_fst, per.name_lst, per.url
            );

            // Fetch addresses into person.
            let url_pathss = [
                vec!["contact/offices"],
                vec!["contact/office-locations"],
                vec!["district"],
                vec!["contact"],
                vec!["offices"],
                vec!["office-locations"],
                vec!["office-information"],
                vec![""],
                vec!["washington-d-c-office", "district-office"],
            ];
            for url_paths in url_pathss {
                if fetch_parse_adrs(&mut self_clone, idx, per, &url_paths, cli, prsr).await? {
                    break;
                }
            }
        }

        Ok(())
    }
}

pub async fn fetch_parse_adrs(
    self_clone: &mut House,
    idx: usize,
    per: &mut Person,
    url_paths: &[&str],
    cli: &Client,
    prsr: &Prsr,
) -> Result<bool> {
    // Fetch one or more pages of adress lines.
    let mut adr_lnes_o: Option<Vec<String>> = None;
    for url_path in url_paths {
        match fetch_adr_lnes(per, url_path, cli, prsr).await? {
            None => {}
            Some(new_lnes) => {
                if adr_lnes_o.is_none() {
                    adr_lnes_o = Some(new_lnes);
                } else {
                    let mut adr_lnes = adr_lnes_o.unwrap();
                    adr_lnes.extend(new_lnes);
                    adr_lnes_o = Some(adr_lnes);
                }
            }
        }
    }

    // Parse lines to Addresses.
    match adr_lnes_o {
        None => return Ok(false),
        Some(mut adr_lnes) => {
            match prsr.parse_addresses(per, &adr_lnes) {
                None => return Ok(false),
                Some(mut adrs) => {
                    filter_invalid_addresses(per, &mut adrs);

                    validate_addresses(per, &adrs)?;

                    eprintln!("{}", AddressList(adrs.clone()));

                    standardize_addresses(&mut adrs, cli).await?;

                    // Write intermediate results to file.
                    // Clone adrs for checkpoint save.
                    let adrs_clone = adrs.clone();
                    self_clone.persons[idx].adrs = Some(adrs_clone);
                    write_to_file(&self_clone, FLE_PTH)?;

                    eprintln!("{}", AddressList(adrs.clone()));

                    per.adrs = Some(adrs);
                }
            }
        }
    }

    Ok(true)
}

pub async fn fetch_adr_lnes(
    per: &mut Person,
    url_path: &str,
    cli: &Client,
    prsr: &Prsr,
) -> Result<Option<Vec<String>>> {
    // Some representative addresses are in a contact webpage.

    // Fetch a URL.
    let mut url = per.url.clone();
    if !url_path.is_empty() {
        url.push('/');
        url.push_str(url_path);
    }
    let html = fetch_html(url.as_str(), cli).await?;

    // Parse HTML.
    let document = Html::parse_document(&html);

    // Attempt to select addresses from various sections of the HTML.
    let mut lnes: Vec<String> = Vec::new();
    for txt in [
        "address",
        "div.address-footer",
        "div.item",
        ".internal__offices--address",
        ".office-locations",
        "article",
        "div.office-address",
        "body",
    ] {
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
                .filter(|s| prsr.filter(s))
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
    edit_person_house_lnes(per, &mut lnes);
    prsr.edit_lnes(&mut lnes);
    edit_newline(&mut lnes);
    edit_hob(&mut lnes);
    edit_split_comma(&mut lnes);
    edit_mailing(&mut lnes);
    edit_starting_hash(&mut lnes);
    edit_char_half(&mut lnes);
    edit_empty(&mut lnes);

    eprintln!("--- post: {lnes:?}");

    if prsr.two_zip_or_more(&lnes) {
        return Ok(Some(lnes));
    }

    Ok(None)
}

pub fn edit_person_house_lnes(per: &Person, lnes: &mut Vec<String>) {
    match (per.name_fst.as_str(), per.name_lst.as_str()) {
        ("Matthew", "Rosendale") => {
            for idx in (0..lnes.len()).rev() {
                if lnes[idx] == "3300 2ND AVENUE N SUITES 7-8" {
                    lnes[idx] = "3300 2ND AVENUE N SUITE 7".into();
                }
            }
        }
        ("Terri", "Sewell") => {
            for idx in (0..lnes.len()).rev() {
                if lnes[idx] == "101 SOUTH LAWRENCE ST COURTHOUSE ANNEX 3" {
                    lnes[idx] = "101 SOUTH LAWRENCE ST".into();
                }
            }
        }
        ("Joe", "Wilson") => {
            for idx in (0..lnes.len()).rev() {
                if lnes[idx] == "1700 SUNSET BLVD (US 378), SUITE 1" {
                    lnes[idx] = "1700 SUNSET BLVD STE 1".into();
                }
            }
        }
        ("Robert", "Wittman") => {
            for idx in (0..lnes.len()).rev() {
                if lnes[idx] == "508 CHURCH LANE" || lnes[idx] == "307 MAIN STREET" {
                    lnes.remove(idx);
                }
            }
        }
        ("Andy", "Biggs") => {
            for idx in (0..lnes.len()).rev() {
                if lnes[idx] == "SUPERSTITION PLAZA" {
                    lnes.remove(idx);
                }
            }
        }
        ("John", "Carter") => {
            for idx in (0..lnes.len()).rev() {
                if lnes[idx] == "SUITE # I-10" {
                    lnes.remove(idx);
                }
            }
        }
        ("Michael", "Cloud") => {
            for idx in (0..lnes.len()).rev() {
                if lnes[idx] == "TOWER II" {
                    lnes.remove(idx);
                }
            }
        }
        ("Tony", "Gonzales") => {
            for idx in (0..lnes.len()).rev() {
                if lnes[idx].contains("(BY APPT ONLY)") {
                    lnes[idx] = lnes[idx].replace(" (BY APPT ONLY)", "");
                }
            }
        }
        ("Garret", "Graves") => {
            for idx in (0..lnes.len()).rev() {
                if lnes[idx].contains("615 E WORTHY STREET GONZALES") {
                    lnes[idx] = "GONZALES".into();
                    lnes.insert(idx, "615 E WORTHY ST".into());
                }
            }
        }
        ("Jared", "Huffman") => {
            for idx in (0..lnes.len()).rev() {
                if lnes[idx] == "430 NORTH FRANKLIN ST FORT BRAGG, CA 95437" {
                    lnes[idx] = "FORT BRAGG, CA 95437".into();
                    lnes.insert(idx, "430 NORTH FRANKLIN ST".into());
                } else if lnes[idx].contains("FORT BRAGG 95437") {
                    lnes[idx] = "FORT BRAGG, CA 95437".into();
                }
            }
        }
        ("Bill", "Huizenga") => {
            for idx in (0..lnes.len()).rev() {
                if lnes[idx].contains("108 PORTAGE, MI 49002") {
                    lnes[idx] = lnes[idx].replace("108 PORTAGE, MI 49002", "108\nPORTAGE, MI 49002")
                }
            }
        }
        ("Mike", "Johnson") => {
            for idx in (0..lnes.len()).rev() {
                if lnes[idx] == "444 CASPARI DRIVE" || lnes[idx] == "SOUTH HALL ROOM 224" {
                    lnes.remove(idx);
                } else if lnes[idx] == "PO BOX 4989 (MAILING)" {
                    lnes[idx] = "PO BOX 4989".into();
                }
            }
        }
        ("Michael", "Lawler") => {
            for idx in (0..lnes.len()).rev() {
                if lnes[idx] == "PO BOX 1645" {
                    lnes.remove(idx);
                }
            }
        }
        ("Anna Paulina", "Luna") => {
            for idx in (0..lnes.len()).rev() {
                if lnes[idx].contains("OFFICE SUITE:") {
                    lnes[idx] = lnes[idx].replace("OFFICE SUITE:", "STE")
                }
            }
        }
        ("Daniel", "Meuser") => {
            for idx in (0..lnes.len()).rev() {
                if lnes[idx] == "SUITE 110, LOSCH PLAZA" {
                    lnes[idx] = "SUITE 110".into();
                }
            }
        }
        ("Max", "Miller") => {
            for idx in (0..lnes.len()).rev() {
                if lnes[idx] == "WASHINGTON" && idx != 0 {
                    lnes.insert(idx - 1, "143 CANNON HOB".into());
                    break;
                }
            }
        }
        ("Frank", "Pallone") => {
            for idx in (0..lnes.len()).rev() {
                if lnes[idx] == "67/69 CHURCH ST" {
                    lnes[idx] = "67 CHURCH ST".into();
                }
            }
        }
        ("Stacey", "Plaskett") => {
            for idx in (0..lnes.len()).rev() {
                if lnes[idx] == "FREDERIKSTED, VI 00840" {
                    lnes[idx] = "ST CROIX, VI 00840".into();
                }
            }
        }
        ("", "") => {}
        _ => {}
    }
}
