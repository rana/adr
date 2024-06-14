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
                    let per = senate.fetch_member(state).await?;
                    senate.persons.push(per);
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
    pub async fn fetch_member(&self, state: &str) -> Result<Person> {
        let url = format!("https://www.senate.gov/states/{state}/intro.htm");
        let html = fetch_html(&url).await?;
        let document = Html::parse_document(&html);
        let mut per = Person::default();

        // Select name and url.
        let name_sel = Selector::parse("div.state-column").expect("Invalid selector");
        let url_sel = Selector::parse("a").expect("Invalid selector");
        for elm_doc in document.select(&name_sel) {
            if let Some(elm_url) = elm_doc.select(&url_sel).next() {
                let full_name = elm_url.text().collect::<Vec<_>>().concat();
                // Select first name and last name.
                // Full name may have middle name or suffix.
                let names = full_name
                    .split_whitespace()
                    .filter(|&w| w != "Jr." && w != "III")
                    .map(|w| w.trim_end_matches(','))
                    .collect::<Vec<_>>();
                per.name_fst = names[0].trim().to_string();
                per.name_lst = names[names.len() - 1].trim().to_string();
                per.url = elm_url
                    .value()
                    .attr("href")
                    .unwrap_or_default()
                    .replace("www.", "")
                    .trim_end_matches('/')
                    .to_string();

                // Validate fields.
                if per.name_fst.is_empty() {
                    return Err(anyhow!("first name empty {:?}", per));
                }
                if per.name_lst.is_empty() {
                    return Err(anyhow!("last name empty {:?}", per));
                }
                if per.url.is_empty() {
                    return Err(anyhow!("url empty {:?}", per));
                }
                if !per.url.ends_with(".senate.gov") {
                    return Err(anyhow!("url doesn't end with '.senate.gov' {:?}", per));
                }
                break;
            }
        }

        if per.name_fst.is_empty() {
            eprintln!("{url}");
            return Err(anyhow!("unable to extract member for {state}"));
        }

        Ok(per)
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
        //.take(1)
        {
            let pct = (((idx as f64 + 1.0) / per_len) * 100.0) as u8;
            eprintln!(
                "  {}% {} {} {} {}",
                pct, idx, per.name_fst, per.name_lst, per.url
            );

            match self.fetch_prs_per(idx, per).await? {
                Some(adrs) => {
                    self.persons[idx].adrs = Some(adrs);
                }
                None => {
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
                        match self.fetch_prs_adrs(per, &url_paths).await? {
                            None => {}
                            Some(adrs) => {
                                self.persons[idx].adrs = Some(adrs);
                                break;
                            }
                        }
                    }
                }
            }

            // Checkpoint save.
            // Write intermediate file to disk.
            write_to_file(&self, FLE_PTH)?;
        }

        Ok(())
    }

    pub async fn fetch_prs_adrs(
        &self,
        per: &Person,
        url_paths: &[&str],
    ) -> Result<Option<Vec<Address>>> {
        // Fetch one or more pages of adress lines.
        let mut adr_lnes_o: Option<Vec<String>> = None;
        for url_path in url_paths {
            match fetch_adr_lnes(per, url_path).await? {
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
        let adrs_o = match adr_lnes_o {
            None => None,
            Some(mut adr_lnes) => match PRSR.prs_adrs(&adr_lnes) {
                None => None,
                Some(mut adrs) => Some(standardize_addresses(adrs).await?),
            },
        };

        Ok(adrs_o)
    }

    pub async fn fetch_prs_per(
        &self,
        idx: usize,
        per: &Person,
    ) -> Result<Option<Vec<Address>>> {
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
            ("", "") => {}
            _ => {}
        }

        Ok(None)
    }
}

pub async fn fetch_adr_lnes(per: &Person, url_path: &str) -> Result<Option<Vec<String>>> {
    // Some representative addresses are in a contact webpage.

    // Fetch a URL.
    let mut url = per.url.clone();
    if !url_path.is_empty() {
        url.push('/');
        url.push_str(url_path);
    }
    let html = fetch_html(url.as_str()).await?;

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
    edit_person_senate_lnes(per, &mut lnes);
    PRSR.edit_lnes(&mut lnes);
    edit_newline(&mut lnes);
    edit_sob(&mut lnes);
    edit_split_comma(&mut lnes);
    edit_mailing(&mut lnes);
    edit_starting_hash(&mut lnes);
    edit_char_half(&mut lnes);
    edit_empty(&mut lnes);

    eprintln!("--- post: {lnes:?}");

    // At least one office in home state, and one in DC.
    if PRSR.two_zip_or_more(&lnes) {
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