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

/// The total number of members in the U.S. House of Representatives is 441. This includes 435 voting members who represent the 50 states and 6 non-voting members who represent the District of Columbia, Puerto Rico, and four other U.S. territories (American Samoa, Guam, the Northern Mariana Islands, and the U.S. Virgin Islands). Some members may be vacant.
const CAP_PER: usize = 441;

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct House {
    pub name: String,
    pub role: Role,
    pub persons: Vec<Person>,
}

impl House {
    pub fn new() -> Self {
        Self {
            name: "U.S. House of Representatives".into(),
            role: Role::Political,
            persons: Vec::new(),
        }
    }

    pub async fn load() -> Result<House> {
        // Read file from disk.
        let mut house = match read_from_file::<House>(FLE_PTH) {
            Ok(mut house_from_disk) => house_from_disk,

            Err(err) => {
                eprintln!("err: read file: {err}");
                let mut house = House::new();

                // Fetch members.
                house.persons = house.fetch_members().await?;

                // Write file to disk.
                write_to_file(&house, FLE_PTH)?;

                house
            }
        };

        println!("{} representatives", house.persons.len());

        // Fetch addresses.
        house.fetch_adrs().await?;

        Ok(house)
    }

    /// Fetch members from network.
    pub async fn fetch_members(&self) -> Result<Vec<Person>> {
        let url = "https://www.house.gov/representatives";
        let html = fetch_html(url).await?;
        let document = Html::parse_document(&html);
        let mut pers = Vec::with_capacity(CAP_PER);

        // Define the CSS selector for the members list
        let tbl_sel = Selector::parse("table.table tr").unwrap();
        let name_sel = Selector::parse("td:nth-of-type(1)").unwrap();
        let url_sel = Selector::parse("td:nth-of-type(1) a").unwrap();

        // Iterate over each member entry
        for tbl_elm in document.select(&tbl_sel) {
            if let Some(name_elm) = tbl_elm.select(&name_sel).next() {
                let mut per = Person::default();
                if let Some((name_lst, name_fst)) = name_elm
                    .text()
                    .collect::<Vec<_>>()
                    .join(" ")
                    .split_once(',')
                {
                    let full_name = format!("{} {}", name_fst.trim(), name_lst.trim());
                    // eprintln!("{}", full_name);
                    per.name = name_clean(&full_name);
                }
                // Skip empty or vacancy.
                // "Mike - Vacancy"
                if per.name.is_empty() || per.name.contains("Vacancy") {
                    continue;
                }
                // Edit some names.
                match per.name.as_str() {
                    "J. Correa" => {
                        per.name = "Lou Correa".into();
                    }
                    "A. Ferguson" => {
                        per.name = "Drew Ferguson".into();
                    }
                    "H. Griffith" => {
                        per.name = "Morgan Griffith".into();
                    }
                    "J. Hill" => {
                        per.name = "French Hill".into();
                    }
                    "C. Ruppersberger" => {
                        per.name = "Dutch Ruppersberger".into();
                    }
                    "W. Steube" => {
                        per.name = "Greg Steube".into();
                    }
                    "" => {}
                    _ => {}
                }

                per.url = tbl_elm.select(&url_sel).next().map_or(String::new(), |a| {
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

                // Validate fields.
                if per.name.is_empty() {
                    return Err(anyhow!("name is empty {:?}", per));
                }
                if per.url.is_empty() {
                    return Err(anyhow!("url is empty {:?}", per));
                }
                if !per.url.ends_with(".house.gov") {
                    return Err(anyhow!("url doesn't end with '.house.gov' {:?}", per));
                }

                // Insert member.
                pers.push(per);
            }
        }

        Ok(pers)
    }

    pub async fn fetch_adrs(&mut self) -> Result<()> {
        // Clone self for file writing.
        let mut self_clone = self.clone();
        let per_len = self.persons.len() as f64;

        for (idx, per) in self_clone
            .persons
            .iter()
            .enumerate()
            .filter(|(_, per)| per.adrs.is_none())
        // .take(1)
        {
            let pct = (((idx as f64 + 1.0) / per_len) * 100.0) as u8;
            eprintln!("  {}% {} {} {}", pct, idx, per.name, per.url);

            match self.persons[idx].name.as_str() {
                // Fetch and gather from multiple urls.
                "Brian Fitzpatrick" => {
                    let mut adrs = Vec::new();
                    for url_path in ["washington-d-c-office", "district-office"] {
                        // Create url.
                        let mut url = per.url.clone();
                        if !url_path.is_empty() {
                            url.push('/');
                            url.push_str(url_path);
                        }
                        // Fetch, parse, standardize.
                        if let Some(new_adrs) = fetch_prs_std_adrs(per, &url).await? {
                            adrs.extend(new_adrs);
                        }
                    }
                    self.persons[idx].adrs = Some(adrs);
                }
                _ => {
                    // Fetch from single unknown url.
                    let url_paths = [
                        "contact/offices",
                        "contact/office-locations",
                        "district",
                        "contact",
                        "offices",
                        "office-locations",
                        "office-information",
                        "",
                    ];
                    for url_path in url_paths {
                        // Create url.
                        let mut url = per.url.clone();
                        if !url_path.is_empty() {
                            url.push('/');
                            url.push_str(url_path);
                        }
                        // Fetch, parse, standardize.
                        if let Some(adrs) = fetch_prs_std_adrs(per, &url).await? {
                            if adrs.len() < 2 {
                                continue;
                            }
                            self.persons[idx].adrs = Some(adrs);
                            break;
                        }
                    }
                }
            }

            // Check for address parsing error.
            if self.persons[idx].adrs.is_none() {
                return Err(anyhow!("no addresses for {}", self.persons[idx]));
            }

            // Checkpoint save.
            write_to_file(&self, FLE_PTH)?;
        }

        Ok(())
    }
}

/// Fetch and parse addresses and standardize with the USPS.
pub async fn fetch_prs_std_adrs(per: &Person, url: &str) -> Result<Option<Vec<Address>>> {
    // Fetch html.
    let html = fetch_html(url).await?;

    // Parse html to address lines.
    let adr_lnes_o = prs_adr_lnes(per, &html);

    // Parse lines to addresses.
    let adrs_o = match adr_lnes_o {
        None => None,
        Some(mut adr_lnes) => match PRSR.prs_adrs(&adr_lnes) {
            None => None,
            Some(mut adrs) => {
                adrs = standardize_addresses(adrs).await?;
                // Can be called twice for different url
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

pub fn prs_adr_lnes(per: &Person, html: &str) -> Option<Vec<String>> {
    let document = Html::parse_document(html);
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

            // eprintln!("--- pre: {cur_lnes:?}");

            // Filter lines.
            // Filter separately to allow debugging.
            cur_lnes = cur_lnes
                .into_iter()
                .filter(|s| PRSR.filter(s))
                .collect::<Vec<String>>();

            if !cur_lnes.is_empty() {
                eprintln!("{cur_lnes:?}");

                lnes.extend(cur_lnes);
            }
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
    edit_person_house_lnes(per, &mut lnes);
    PRSR.edit_lnes(&mut lnes);
    edit_newline(&mut lnes);
    edit_hob(&mut lnes);
    edit_split_comma(&mut lnes);
    edit_starting_hash(&mut lnes);
    edit_char_half(&mut lnes);
    edit_empty(&mut lnes);

    eprintln!("--- --- --- post: {lnes:?}");

    // Do not check for zip count here.

    Some(lnes)
}

pub fn edit_person_house_lnes(per: &Person, lnes: &mut Vec<String>) {
    match per.name.as_str() {
        "Matthew Rosendale" => {
            for idx in (0..lnes.len()).rev() {
                if lnes[idx] == "3300 2ND AVENUE N SUITES 7-8" {
                    lnes[idx] = "3300 2ND AVENUE N SUITE 7".into();
                }
            }
        }
        "Terri Sewell" => {
            for idx in (0..lnes.len()).rev() {
                if lnes[idx] == "101 SOUTH LAWRENCE ST COURTHOUSE ANNEX 3" {
                    lnes[idx] = "101 SOUTH LAWRENCE ST".into();
                }
            }
        }
        "Joe Wilson" => {
            for idx in (0..lnes.len()).rev() {
                if lnes[idx] == "1700 SUNSET BLVD (US 378), SUITE 1" {
                    lnes[idx] = "1700 SUNSET BLVD STE 1".into();
                }
            }
        }
        "Robert Wittman" => {
            for idx in (0..lnes.len()).rev() {
                if lnes[idx] == "508 CHURCH LANE" || lnes[idx] == "307 MAIN STREET" {
                    lnes.remove(idx);
                }
            }
        }
        "Andy Biggs" => {
            for idx in (0..lnes.len()).rev() {
                if lnes[idx] == "SUPERSTITION PLAZA" {
                    lnes.remove(idx);
                }
            }
        }
        "John Carter" => {
            for idx in (0..lnes.len()).rev() {
                if lnes[idx] == "SUITE # I-10" {
                    lnes.remove(idx);
                }
            }
        }
        "Michael Cloud" => {
            for idx in (0..lnes.len()).rev() {
                if lnes[idx] == "TOWER II, SUITE 980" {
                    lnes[idx] = "SUITE 980".into();
                }
            }
        }
        "Tony Gonzales" => {
            for idx in (0..lnes.len()).rev() {
                if lnes[idx].contains("(BY APPT ONLY)") {
                    lnes[idx] = lnes[idx].replace(" (BY APPT ONLY)", "");
                }
            }
        }
        "Garret Graves" => {
            for idx in (0..lnes.len()).rev() {
                if lnes[idx].contains("615 E WORTHY STREET GONZALES") {
                    lnes[idx] = "GONZALES".into();
                    lnes.insert(idx, "615 E WORTHY ST".into());
                }
            }
        }
        "Jared Huffman" => {
            for idx in (0..lnes.len()).rev() {
                if lnes[idx] == "430 NORTH FRANKLIN ST FORT BRAGG, CA 95437" {
                    lnes[idx] = "FORT BRAGG, CA 95437".into();
                    lnes.insert(idx, "430 NORTH FRANKLIN ST".into());
                } else if lnes[idx].contains("FORT BRAGG 95437") {
                    lnes[idx] = "FORT BRAGG, CA 95437".into();
                }
            }
        }
        "Bill Huizenga" => {
            for idx in (0..lnes.len()).rev() {
                if lnes[idx].contains("108 PORTAGE, MI 49002") {
                    lnes[idx] = lnes[idx].replace("108 PORTAGE, MI 49002", "108\nPORTAGE, MI 49002")
                }
            }
        }
        "Mike Johnson" => {
            for idx in (0..lnes.len()).rev() {
                if lnes[idx] == "444 CASPARI DRIVE" {
                    lnes.remove(idx + 2);
                    lnes.remove(idx + 1);
                    lnes.remove(idx);
                } else if lnes[idx] == "PO BOX 4989 (MAILING)" {
                    lnes[idx] = "PO BOX 4989".into();
                } else if lnes[idx] == "PO BOX 779 (MAILING)" {
                    lnes[idx] = "PO BOX 779".into();
                }
            }
        }
        "Michael Lawler" => {
            for idx in (0..lnes.len()).rev() {
                if lnes[idx] == "PO BOX 1645" {
                    lnes.remove(idx);
                }
            }
        }
        "Anna Paulina Luna" => {
            for idx in (0..lnes.len()).rev() {
                if lnes[idx].contains("OFFICE SUITE:") {
                    lnes[idx] = lnes[idx].replace("OFFICE SUITE:", "STE")
                }
            }
        }
        "Daniel Meuser" => {
            for idx in (0..lnes.len()).rev() {
                if lnes[idx] == "SUITE 110, LOSCH PLAZA" {
                    lnes[idx] = "SUITE 110".into();
                }
            }
        }
        "Max Miller" => {
            for idx in (0..lnes.len()).rev() {
                if lnes[idx] == "WASHINGTON" && idx != 0 {
                    lnes.insert(idx - 1, "143 CHOB".into());
                    break;
                }
            }
        }
        "Frank Pallone" => {
            for idx in (0..lnes.len()).rev() {
                if lnes[idx] == "67/69 CHURCH ST" {
                    lnes[idx] = "67 CHURCH ST".into();
                }
            }
        }
        "Stacey Plaskett" => {
            for idx in (0..lnes.len()).rev() {
                if lnes[idx] == "FREDERIKSTED, VI 00840" {
                    lnes[idx] = "ST CROIX, VI 00840".into();
                }
            }
        }
        "Raul Grijalva" => {
            for idx in (0..lnes.len()).rev() {
                // "146 N. STATE AVENUE", "SOMERTON AZ 85350"
                if lnes[idx] == "146 N STATE AVENUE" {
                    lnes.remove(idx + 1);
                    lnes.remove(idx);
                } else if lnes[idx].starts_with("MAILING ADDRESS") {
                    // "MAILING ADDRESS: PO BOX", "4105, SOMERTON, AZ 85350"
                    let mut lne = lnes.remove(idx + 1);
                    lne.insert_str(0, "PO BOX ");
                    lnes[idx] = lne;
                } else if lnes[idx].starts_with("EL PUEBLO") {
                    // "EL PUEBLO COMMUNITY CENTER"
                    lnes.remove(idx);
                }
            }
        }
        "Bryan Steil" => {
            for idx in (0..lnes.len()).rev() {
                if lnes[idx].contains("CIVIC CENTER") {
                    // "ST FRANCIS CIVIC CENTER"
                    lnes.remove(idx);
                }
            }
        }
        "Kevin Kiley" => {
            for idx in (0..lnes.len()).rev() {
                if lnes[idx] == "33 SOUTH MAIN STREET" {
                    lnes.remove(idx + 3);
                    lnes.remove(idx + 2);
                    lnes.remove(idx + 1);
                    lnes.remove(idx);
                }
            }
        }
        "" => {}
        _ => {}
    }
}
