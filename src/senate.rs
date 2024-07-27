use crate::core::*;
use crate::models::*;
use crate::prsr::*;
use crate::usps::*;
use anyhow::{anyhow, Result};
use scraper::{Html, Selector};
use serde::{Deserialize, Serialize};
use std::fs::File;
use std::io::{BufReader, BufWriter};
use std::ops::Add;
use std::path::Path;

const FLE_PTH: &str = "senate.json";

/// The U.S. Senate consists of 100 members, with each of the 50 states represented by two senators regardless of population size.
const CAP_PER: usize = 100;

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Senate {
    pub name: String,
    pub role: Role,
    pub persons: Vec<Person>,
}

impl Senate {
    pub fn new() -> Self {
        Self {
            name: "U.S. Senate".into(),
            role: Role::Political,
            persons: Vec::with_capacity(CAP_PER),
        }
    }

    pub async fn load() -> Result<Senate> {
        // Read file from disk.
        let mut senate = match read_from_file::<Senate>(FLE_PTH) {
            Ok(senate_from_disk) => senate_from_disk,
            Err(_) => {
                let mut senate = Senate::new();

                // Fetch members.
                let states = vec![
                    "AL", "AK", "AZ", "AR", "CA", "CO", "CT", "DE", "FL", "GA", "HI", "ID", "IL",
                    "IN", "IA", "KS", "KY", "LA", "ME", "MD", "MA", "MI", "MN", "MS", "MO", "MT",
                    "NE", "NV", "NH", "NJ", "NM", "NY", "NC", "ND", "OH", "OK", "OR", "PA", "RI",
                    "SC", "SD", "TN", "TX", "UT", "VT", "VA", "WA", "WV", "WI", "WY",
                ];
                for state in states {
                    let pers = senate.fetch_members(state).await?;
                    senate.persons.extend(pers);
                }

                // Write file to disk.
                write_to_file(&senate, FLE_PTH)?;

                senate
            }
        };

        println!("{} senators", senate.persons.len());

        // Fetch addresses.
        senate.fetch_adrs().await?;

        Ok(senate)
    }

    /// Fetch member from network.
    pub async fn fetch_members(&self, state: &str) -> Result<Vec<Person>> {
        let url = format!("https://www.senate.gov/states/{state}/intro.htm");
        let html = fetch_html(&url).await?;
        let document = Html::parse_document(&html);

        let mut pers = Vec::new();

        // Select name and url.
        let name_sel = Selector::parse("div.state-column").expect("Invalid selector");
        let url_sel = Selector::parse("a").expect("Invalid selector");
        for elm_doc in document.select(&name_sel) {
            if let Some(elm_url) = elm_doc.select(&url_sel).next() {
                let mut per = Person::default();
                let full_name = elm_url.text().collect::<Vec<_>>().concat();
                eprintln!("{}", full_name.trim());
                per.name = name_clean(&full_name);
                per.url = elm_url
                    .value()
                    .attr("href")
                    .unwrap_or_default()
                    .replace("www.", "")
                    .trim_end_matches('/')
                    .to_string();

                // Validate fields.
                if per.name.is_empty() {
                    return Err(anyhow!("name is empty {:?}", per));
                }
                if per.url.is_empty() {
                    return Err(anyhow!("url is empty {:?}", per));
                }
                if !per.url.ends_with(".senate.gov") {
                    return Err(anyhow!("url doesn't end with '.senate.gov' {:?}", per));
                }

                pers.push(per);
            }
        }

        if pers.len() != 2 {
            return Err(anyhow!("missing two senators for {state}"));
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

            match self.fetch_prs_per(idx, per).await? {
                Some(adrs) => {
                    self.persons[idx].adrs = Some(adrs);
                }
                None => {
                    // Fetch from single unknown url.
                    let url_paths = [
                        "contact",
                        "contact/offices",
                        "",
                        "public",
                        "public/index.cfm/office-locations",
                        "contact/office-locations",
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
            // Write intermediate file to disk.
            write_to_file(&self, FLE_PTH)?;
        }

        Ok(())
    }

    pub async fn fetch_prs_per(&self, idx: usize, per: &Person) -> Result<Option<Vec<Address>>> {
        match per.name.as_str() {
            "John W. Hickenlooper" => {
                let url = "https://hickenlooper.senate.gov/wp-json/wp/v2/locations";
                let response = reqwest::get(url).await?.text().await?;
                let locations: Vec<Location> = serde_json::from_str(&response)?;
                let mut adrs: Vec<Address> = locations
                    .into_iter()
                    .map(|loc| {
                        let mut adr = Address {
                            address1: loc.acf.address,
                            address2: if loc.acf.suite.is_empty() {
                                None
                            } else {
                                Some(loc.acf.suite)
                            },
                            city: loc.acf.city,
                            state: loc.acf.state,
                            ..Default::default()
                        };
                        let lne_zip = &loc.acf.zipcode;
                        let is_zip5 = is_zip5(lne_zip);
                        let is_zip10 = if !is_zip5 { is_zip10(lne_zip) } else { false };
                        if is_zip5 {
                            adr.zip5 = lne_zip.parse().unwrap();
                        } else {
                            adr.zip5 = lne_zip[..5].parse().unwrap();
                            adr.zip4 = lne_zip[lne_zip.len() - 4..].parse().unwrap();
                        }

                        adr
                    })
                    .collect();
                for idx in (0..adrs.len()).rev() {
                    if adrs[idx].address1 == "~" {
                        adrs.remove(idx);
                    } else if adrs[idx].address1.starts_with("2 Constitution Ave") {
                        // Russell Senate Office Building
                        // 2 Constitution Ave NE,Suite SR-374
                        if let Some(adr2) = adrs[idx].address2.clone() {
                            if let Some(idx_fnd) = adr2.find("SR-") {
                                let mut adr1 = adr2[idx_fnd + 3..].to_string();
                                adr1.push_str(" RUSSELL SOB");
                                adrs[idx].address1 = adr1;
                                adrs[idx].address2 = None;
                            }
                        }
                    }
                }
                return Ok(Some(standardize_addresses(adrs).await?));
            }
            "" => {}
            _ => {}
        }

        Ok(None)
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
                if adrs.len() < 2 {
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
        "li",
        "div.et_pb_blurb_description",
        "div.et_pb_promo_description",
        "div.OfficeLocations__addressText",
        "div.map-office-box",
        "div.et_pb_text_inner",
        "div.location-content-inner",
        "div.address",
        "address",
        "div.address-footer",
        "div.counties_listing",
        "div.location-info",
        "div.item",
        ".internal__offices--address",
        ".office-locations",
        "div.office-address",
        "body",
    ] {
        let selector = Selector::parse(txt).unwrap();
        for elm in document.select(&selector) {
            let mut cur_lnes: Vec<String>;

            // Extract lines from html.
            if txt == "li" {
                // For Marco Rubio
                cur_lnes = vec![
                    elm.value()
                        .attr("data-addr")
                        .unwrap_or_default()
                        .trim()
                        .trim_end_matches(',')
                        .to_uppercase()
                        .to_string(),
                    elm.value()
                        .attr("data-city")
                        .unwrap_or_default()
                        .trim()
                        .trim_end_matches(',')
                        .to_uppercase()
                        .to_string(),
                ];
            } else {
                cur_lnes = elm
                    .text()
                    .map(|s| s.trim().trim_end_matches(',').to_uppercase().to_string())
                    .collect::<Vec<String>>();
            }

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
    edit_person_senate_lnes(per, &mut lnes);
    PRSR.edit_lnes(&mut lnes);
    edit_newline(&mut lnes);
    edit_sob(&mut lnes);
    edit_split_comma(&mut lnes);
    edit_starting_hash(&mut lnes);
    edit_char_half(&mut lnes);
    edit_empty(&mut lnes);

    eprintln!("--- --- --- post: {lnes:?}");

    // Do not check for zip count here.

    Some(lnes)
}

pub fn edit_person_senate_lnes(per: &Person, lnes: &mut Vec<String>) {
    match per.name.as_str() {
        "Tommy Tuberville" => {
            for idx in (0..lnes.len()).rev() {
                if lnes[idx] == "BB&T CENTRE 41 WEST I-65" {
                    lnes[idx] = "41 W I-65 SERVICE RD N STE 2300-A".into();
                    lnes.remove(idx + 1);
                }
            }
        }
        "Chuck Grassley" => {
            for idx in (0..lnes.len()).rev() {
                if lnes[idx] == "210 WALNUT STREET" {
                    lnes.remove(idx);
                }
            }
        }
        "Joni Ernst" => {
            for idx in (0..lnes.len()).rev() {
                if lnes[idx] == "2146 27" {
                    lnes[idx] = "2146 27TH AVE".into();
                    lnes.remove(idx + 1);
                    lnes.remove(idx + 1);
                } else if lnes[idx] == "210 WALNUT STREET" {
                    lnes.remove(idx);
                }
            }
        }
        "Roger Marshall" => {
            for idx in (0..lnes.len()).rev() {
                if lnes[idx].contains("20002") {
                    lnes[idx] = lnes[idx].replace("20002", "20510");
                }
            }
        }
        "Benjamin L. Cardin" => {
            for idx in (0..lnes.len()).rev() {
                if lnes[idx] == "TOWER 1, SUITE 1710" {
                    lnes[idx] = "SUITE 1710".into();
                }
            }
        }
        "Jeanne Shaheen" => {
            for idx in (0..lnes.len()).rev() {
                if lnes[idx] == "OFFICE BUILDING" {
                    lnes.remove(idx);
                }
            }
        }
        "Robert Menendez" => {
            for idx in (0..lnes.len()).rev() {
                if lnes[idx] == "HARBORSIDE 3, SUITE 1000" {
                    lnes[idx] = "SUITE 1000".into();
                }
            }
        }
        "Martin Heinrich" => {
            // "709 HART SENATE OFFICE BUILDING WASHINGTON, D.C. 20510"
            for idx in (0..lnes.len()).rev() {
                if lnes[idx].starts_with("709 HART") {
                    lnes[idx] = "709 HART SOB, WASHINGTON, DC 20510".into();
                }
            }
        }

        "Charles E. Schumer" => {
            // "LEO O'BRIEN BUILDING, ROOM 827"
            for idx in (0..lnes.len()).rev() {
                if lnes[idx].starts_with("LEO O'BRIEN") {
                    lnes[idx] = "1 CLINTON SQ STE 827".into();
                }
            }
        }
        "Kevin Cramer" => {
            // "328 FEDERAL BUILDING", "220 EAST ROSSER AVENUE"
            for idx in (0..lnes.len()).rev() {
                if lnes[idx] == "328 FEDERAL BUILDING" {
                    let lne = lnes[idx].clone();
                    let digits = lne.split_whitespace().next().unwrap();
                    lnes.remove(idx);
                    lnes[idx].push_str(" RM ");
                    lnes[idx].push_str(digits);
                }
            }
        }
        "Sheldon Whitehouse" => {
            // "HART SENATE OFFICE BLDG., RM. 530"
            for idx in (0..lnes.len()).rev() {
                if lnes[idx].starts_with("HART SENATE") {
                    lnes[idx] = "530 HART SOB".into();
                }
            }
        }
        "John Thune" => {
            // "UNITED STATES SENATE SD-511"
            for idx in (0..lnes.len()).rev() {
                if lnes[idx] == "UNITED STATES SENATE SD-511" {
                    lnes[idx] = "511 DIRKSEN SOB".into();
                }
            }
        }
        "Mike Rounds" => {
            // "HART SENATE OFFICE BLDG., SUITE 716"
            for idx in (0..lnes.len()).rev() {
                if lnes[idx].starts_with("HART SENATE") {
                    lnes[idx] = "716 HART SOB".into();
                }
            }
        }
        "Marsha Blackburn" => {
            // "10 WEST M. L. KING BLVD"
            for idx in (0..lnes.len()).rev() {
                if lnes[idx].starts_with("10 WEST M") {
                    lnes[idx] = "10 MARTIN LUTHER KING BLVD".into();
                }
            }
        }
        "Bill Hagerty" => {
            // "109 S.HIGHLAND AVENUE"
            for idx in (0..lnes.len()).rev() {
                if lnes[idx].starts_with("109 S") {
                    lnes[idx] = "109 S HIGHLAND AVE".into();
                } else if lnes[idx] == "20002" {
                    lnes[idx] = "20510".into();
                }
            }
        }
        "Ted Cruz" => {
            // "MICKEY LELAND FEDERAL BLDG. 1919 SMITH ST., SUITE 9047"
            for idx in (0..lnes.len()).rev() {
                if lnes[idx].starts_with("MICKEY LELAND FEDERAL") {
                    lnes[idx] = "1919 SMITH ST STE 9047".into();
                } else if lnes[idx] == "167 RUSSELL" {
                    lnes[idx].push_str(" SOB");
                }
            }
        }
        "Peter Welch" => {
            // SR-124 RUSSELL
            for idx in (0..lnes.len()).rev() {
                if lnes[idx].starts_with("SR-124 RUSSELL") {
                    lnes[idx] = lnes[idx][3..].into();
                }
            }
        }
        "John Barrasso" => {
            // "1575 DEWAR DRIVE (COMMERCE BANK)"
            for idx in (0..lnes.len()).rev() {
                if lnes[idx].ends_with("(COMMERCE BANK)") {
                    lnes[idx] = "1575 DEWAR DR".into();
                }
            }
        }
        "Cynthia M. Lummis" => {
            for idx in (0..lnes.len()).rev() {
                if lnes[idx].starts_with("RUSSELL SENATE") {
                    // "RUSSELL SENATE OFFICE BUILDING SUITE SR-127A WASHINGTON, DC 20510"
                    lnes[idx] = "127 RUSSELL SOB".into();
                    lnes.insert(idx + 1, "WASHINGTON, DC 20510".into());
                } else if lnes[idx].starts_with("FEDERAL CENTER") {
                    // "FEDERAL CENTER 2120 CAPITOL AVENUE SUITE 2007 CHEYENNE, WY 82001"
                    lnes[idx] = "2120 CAPITOL AVE STE 2007".into();
                    lnes.insert(idx + 1, "CHEYENNE, WY 82001".into());
                }
            }
        }
        "Jon Tester" => {
            for idx in (0..lnes.len()).rev() {
                if lnes[idx] == "SILVER BOW CENTER" {
                    lnes.remove(idx);
                }
            }
        }
        "John Cornyn" => {
            for idx in (0..lnes.len()).rev() {
                if lnes[idx] == "WELLS FARGO CENTER" {
                    lnes.remove(idx);
                }
            }
        }
        "" => {}
        _ => {}
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct Location {
    acf: LocationAcf,
}
#[derive(Debug, Serialize, Deserialize)]
struct LocationAcf {
    address: String,
    suite: String,
    city: String,
    state: String,
    zipcode: String,
}
