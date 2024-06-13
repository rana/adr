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

const FLE_PTH: &str = "senate.json";

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Senate {
    pub name: String,
    pub role: Role,
    pub persons: Vec<Person>,
}

impl Senate {
    pub fn new() -> Self {
        // The U.S. Senate consists of 100 members, with each of the 50 states represented by two senators regardless of population size.
        Self {
            name: "U.S. Senate".into(),
            role: Role::Political,
            persons: Vec::with_capacity(441),
        }
    }

    pub async fn load() -> Result<Senate> {
        // Read file from disk.
        let senate = match read_from_file::<Senate>(FLE_PTH) {
            Ok(senate_from_disk) => {
                // Read from disk.
                senate_from_disk
            }
            Err(err) => {
                // File not available.
                eprintln!("read file {}: err: {}", FLE_PTH, err);

                let mut senate = Senate::new();

                // Fetch list of members from network.
                // Fetch two senators from each senate state pagge.
                let cli = Client::new();
                let states = vec![
                    "AL", "AK", "AZ", "AR", "CA", "CO", "CT", "DE", "FL", "GA", "HI", "ID", "IL",
                    "IN", "IA", "KS", "KY", "LA", "ME", "MD", "MA", "MI", "MN", "MS", "MO", "MT",
                    "NE", "NV", "NH", "NJ", "NM", "NY", "NC", "ND", "OH", "OK", "OR", "PA", "RI",
                    "SC", "SD", "TN", "TX", "UT", "VT", "VA", "WA", "WV", "WI", "WY",
                ];
                for state in states {
                    let url = format!("https://www.senate.gov/states/{state}/intro.htm");
                    let html = fetch_html(&url, &cli).await?;

                    // Extract members from html.
                    senate.extract_members(&html);
                }

                // Validate member fields.
                senate.validate_members()?;

                // Write file to disk.
                write_to_file(&senate, FLE_PTH)?;

                senate
            }
        };

        println!("{} senators", senate.persons.len());

        Ok(senate)
    }

    /// Extract members from the specified html.
    pub fn extract_members(&mut self, html: &str) {
        let document = Html::parse_document(html);

        // Define the selector for the "div.state-column"
        let state_column_selector = Selector::parse("div.state-column").expect("Invalid selector");

        // Define the selector for extracting the full name and URL within the "div.state-column"
        let link_selector = Selector::parse("a").expect("Invalid selector");

        // Iterate over the "div.state-column" elements and extract the required information
        for element in document.select(&state_column_selector) {
            if let Some(link_element) = element.select(&link_selector).next() {
                let full_name = link_element.text().collect::<Vec<_>>().concat();
                // Select first name and last name.
                // Full name may have middle name or suffix.
                let names = full_name
                    .split_whitespace()
                    .filter(|&w| w != "Jr." && w != "III")
                    .map(|w| w.trim_end_matches(','))
                    .collect::<Vec<_>>();
                let name_fst = names[0].trim().to_string();
                let name_lst = names[names.len() - 1].trim().to_string();
                let url = link_element
                    .value()
                    .attr("href")
                    .unwrap_or_default()
                    .replace("www.", "")
                    .trim_end_matches('/')
                    .to_string();
                let per = Person {
                    name_fst,
                    name_lst,
                    url,
                    ..Default::default()
                };
                eprintln!("{per}");
                self.persons.push(per);
            }
        }
    }

    pub fn validate_members(&self) -> Result<()> {
        for (idx, rep) in self.persons.iter().enumerate() {
            if rep.name_fst.is_empty() {
                return Err(anyhow!("senate: first name empty (idx:{} {:?})", idx, rep));
            }
            if rep.name_lst.is_empty() {
                return Err(anyhow!("senate: last name empty (idx:{} {:?})", idx, rep));
            }
            if rep.url.is_empty() {
                return Err(anyhow!("senate: url empty (idx:{} {:?})", idx, rep));
            }
            if !rep.url.ends_with(".senate.gov") {
                return Err(anyhow!(
                    "senate: url doesn't end with '.senate.gov' (idx:{} {:?})",
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
        for (idx, mut per) in self
            .persons
            .iter_mut()
            .enumerate()
            .filter(|(_, per)| per.adrs.is_none())
            //.take(1)
        {
            let pct = (((idx as f64 + 1.0) / per_len) * 100.0) as u8;
            eprintln!(
                "  {}% {} {} {} {}",
                pct, idx, per.name_fst, per.name_lst, per.url
            );

            match fetch_and_parse_person(&mut self_clone, idx, per, cli).await {
                Err(err) => {
                    return Err(err);
                }
                Ok(fetched_adrs) => {
                    if !fetched_adrs {
                        // Fetch addresses into person.
                        let url_pathss = [
                            vec!["contact"],
                            vec!["contact/locations"],
                            vec!["contact/offices"],
                            vec!["contact/office-locations"],
                            vec!["office-locations"],
                            vec!["offices"],
                            vec!["office-locations"],
                            vec!["office-information"],
                            vec![""],
                            vec!["public"],
                            vec!["public/index.cfm/office-locations"],
                        ];
                        for url_paths in url_pathss {
                            if fetch_parse_adrs(&mut self_clone, idx, per, &url_paths, cli, prsr)
                                .await?
                            {
                                break;
                            }
                        }
                    }
                }
            }
        }

        Ok(())
    }
}

pub async fn fetch_parse_adrs(
    self_clone: &mut Senate,
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
                    // filter_invalid_addresses(per, &mut adrs);

                    // validate_addresses(per, &adrs)?;

                    // eprintln!("{}", AddressList(adrs.clone()));

                    // standardize_addresses(&mut adrs, cli).await?;

                    // // Write intermediate results to file.
                    // // Clone adrs for checkpoint save.
                    // let adrs_clone = adrs.clone();
                    // self_clone.persons[idx].adrs = Some(adrs_clone);
                    // write_to_file(&self_clone, FLE_PTH)?;

                    // eprintln!("{}", AddressList(adrs.clone()));

                    // per.adrs = Some(adrs);
                    process_parsed_addresses(self_clone, idx, per, &mut adrs, cli).await?;
                }
            }
        }
    }

    Ok(true)
}

pub async fn process_parsed_addresses(
    self_clone: &mut Senate,
    idx: usize,
    per: &mut Person,
    mut adrs: &mut Vec<Address>,
    cli: &Client,
) -> Result<()> {
    filter_invalid_addresses(per, adrs);

    validate_addresses(per, adrs)?;

    eprintln!("{}", AddressList(adrs.clone()));

    standardize_addresses(adrs, cli).await?;

    // Write intermediate results to file.
    // Clone adrs for checkpoint save.
    let adrs_clone = adrs.clone();
    self_clone.persons[idx].adrs = Some(adrs_clone);
    write_to_file(&self_clone, FLE_PTH)?;

    eprintln!("{}", AddressList(adrs.clone()));

    per.adrs = Some(adrs.to_vec());

    Ok(())
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
        "div.et_pb_blurb_description",
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
    edit_nbsp(&mut lnes);
    edit_person_senate_lnes(per, &mut lnes);
    prsr.edit_lnes(&mut lnes);
    edit_newline(&mut lnes);
    edit_sob(&mut lnes);
    edit_split_comma(&mut lnes);
    edit_mailing(&mut lnes);
    edit_starting_hash(&mut lnes);
    edit_char_half(&mut lnes);
    edit_empty(&mut lnes);

    eprintln!("--- post: {lnes:?}");

    // At least one office in home state, and one in DC.
    if prsr.two_zip_or_more(&lnes) {
        return Ok(Some(lnes));
    }

    Ok(None)
}

pub fn edit_person_senate_lnes(per: &Person, lnes: &mut Vec<String>) {
    match (per.name_fst.as_str(), per.name_lst.as_str()) {
        ("Tommy", "Tuberville") => {
            for idx in (0..lnes.len()).rev() {
                if lnes[idx] == "BB&T CENTRE 41 WEST I-65" {
                    lnes[idx] = "41 W I-65 SERVICE RD N STE 2300-A".into();
                    lnes.remove(idx + 1);
                }
            }
        }
        ("Chuck", "Grassley") => {
            for idx in (0..lnes.len()).rev() {
                if lnes[idx] == "210 WALNUT STREET" {
                    lnes.remove(idx);
                }
            }
        }
        ("Joni", "Ernst") => {
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
        ("Roger", "Marshall") => {
            for idx in (0..lnes.len()).rev() {
                if lnes[idx].contains("20002") {
                    lnes[idx] = lnes[idx].replace("20002", "20510");
                }
            }
        }
        ("Angus", "King") => {
            for idx in (0..lnes.len()).rev() {
                if lnes[idx].starts_with("40 WESTERN AVE") {
                    lnes[idx] = "40 WESTERN AVE UNIT 412".into();
                }
            }
        }
        ("Benjamin", "Cardin") => {
            for idx in (0..lnes.len()).rev() {
                if lnes[idx] == "TOWER 1, SUITE 1710" {
                    lnes[idx] = "SUITE 1710".into();
                }
            }
        }
        ("Jeanne", "Shaheen") => {
            for idx in (0..lnes.len()).rev() {
                if lnes[idx] == "OFFICE BUILDING" {
                    lnes.remove(idx);
                }
            }
        }
        ("Robert", "Menendez") => {
            for idx in (0..lnes.len()).rev() {
                if lnes[idx] == "HARBORSIDE 3, SUITE 1000" {
                    lnes[idx] = "SUITE 1000".into();
                }
            }
        }
        ("Martin", "Heinrich") => {
            // "709 HART SENATE OFFICE BUILDING WASHINGTON, D.C. 20510"
            for idx in (0..lnes.len()).rev() {
                if lnes[idx].starts_with("709 HART") {
                    lnes[idx] = "709 HART SOB, WASHINGTON, DC 20510".into();
                }
            }
        }

        ("Charles", "Schumer") => {
            // "LEO O'BRIEN BUILDING, ROOM 827"
            for idx in (0..lnes.len()).rev() {
                if lnes[idx].starts_with("LEO O'BRIEN") {
                    lnes[idx] = "1 CLINTON SQ STE 827".into();
                }
            }
        }
        ("Kevin", "Cramer") => {
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
        ("Sheldon", "Whitehouse") => {
            // "HART SENATE OFFICE BLDG., RM. 530"
            for idx in (0..lnes.len()).rev() {
                if lnes[idx].starts_with("HART SENATE") {
                    lnes[idx] = "530 HART SOB".into();
                }
            }
        }
        ("John", "Thune") => {
            // "UNITED STATES SENATE SD-511"
            for idx in (0..lnes.len()).rev() {
                if lnes[idx] == "UNITED STATES SENATE SD-511" {
                    lnes[idx] = "511 DIRKSEN SOB".into();
                }
            }
        }
        ("Mike", "Rounds") => {
            // "HART SENATE OFFICE BLDG., SUITE 716"
            for idx in (0..lnes.len()).rev() {
                if lnes[idx].starts_with("HART SENATE") {
                    lnes[idx] = "716 HART SOB".into();
                }
            }
        }
        ("Marsha", "Blackburn") => {
            // "10 WEST M. L. KING BLVD"
            for idx in (0..lnes.len()).rev() {
                if lnes[idx].starts_with("10 WEST M") {
                    lnes[idx] = "10 MARTIN LUTHER KING BLVD".into();
                }
            }
        }
        ("Bill", "Hagerty") => {
            // "109 S.HIGHLAND AVENUE"
            for idx in (0..lnes.len()).rev() {
                if lnes[idx].starts_with("109 S") {
                    lnes[idx] = "109 S HIGHLAND AVE".into();
                } else if lnes[idx] == "20002" {
                    lnes[idx] = "20510".into();
                }
            }
        }
        ("Ted", "Cruz") => {
            // "MICKEY LELAND FEDERAL BLDG. 1919 SMITH ST., SUITE 9047"
            for idx in (0..lnes.len()).rev() {
                if lnes[idx].starts_with("MICKEY LELAND FEDERAL") {
                    lnes[idx] = "1919 SMITH ST STE 9047".into();
                } else if lnes[idx] == "167 RUSSELL" {
                    lnes[idx].push_str(" SOB");
                }
            }
        }
        ("Peter", "Welch") => {
            // SR-124 RUSSELL
            for idx in (0..lnes.len()).rev() {
                if lnes[idx].starts_with("SR-124 RUSSELL") {
                    lnes[idx] = lnes[idx][3..].into();
                }
            }
        }
        ("John", "Barrasso") => {
            // "1575 DEWAR DRIVE (COMMERCE BANK)"
            for idx in (0..lnes.len()).rev() {
                if lnes[idx].ends_with("(COMMERCE BANK)") {
                    lnes[idx] = "1575 DEWAR DR".into();
                }
            }
        }
        ("Cynthia", "Lummis") => {
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
        ("", "") => {}
        _ => {}
    }
}

pub async fn fetch_and_parse_person(
    self_clone: &mut Senate,
    idx: usize,
    per: &mut Person,
    cli: &Client,
) -> Result<bool> {
    match (per.name_fst.as_str(), per.name_lst.as_str()) {
        ("John", "Hickenlooper") => {
            let url = "https://hickenlooper.senate.gov/wp-json/wp/v2/locations";
            let response = reqwest::get(url).await?.text().await?;
            let locations: Vec<Location> = serde_json::from_str(&response)?;
            let mut adrs: Vec<Address> = locations
                .into_iter()
                .map(|location| location.acf.into())
                .collect();
            for idx in (0..adrs.len()).rev() {
                if adrs[idx].address1 == "~" {
                    adrs.remove(idx);
                } else if adrs[idx].address1.starts_with("2 Constitution Ave") {
                    if let Some(adr2) = adrs[idx].address2.clone() {
                        if let Some(idx_fnd) = adr2.find("SR-") {
                            let mut adr1 = adr2[idx_fnd + 3..].to_string();
                            adr1.push_str(" RUSSELL SOB");
                            adrs[idx].address1 = adr1;
                            adrs[idx].address2 = None;
                        }
                    }
                }
                // Russell Senate Office Building
                // 2 Constitution Ave NE,Suite SR-374
            }
            process_parsed_addresses(self_clone, idx, per, &mut adrs, cli).await?;
            return Ok(true);
        }
        ("", "") => {}
        _ => {}
    }

    Ok(false)
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
impl From<LocationAcf> for Address {
    fn from(acf: LocationAcf) -> Self {
        Address {
            address1: acf.address,
            address2: if acf.suite.is_empty() {
                None
            } else {
                Some(acf.suite)
            },
            city: acf.city,
            state: acf.state,
            zip: acf.zipcode,
        }
    }
}
