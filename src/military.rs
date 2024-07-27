use crate::core::*;
use crate::models::*;
use crate::prsr::*;
use crate::usps::*;
use anyhow::{anyhow, Result};
use heck::ToTitleCase;
use reqwest::Client;
use scraper::{Html, Selector};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt;
use std::fs::File;
use std::io::{BufReader, BufWriter};
use std::ops::Add;
use std::path::Path;
use strum::EnumIter; // Required to derive EnumIter
use strum::IntoEnumIterator;
use Center::*; // Required for iterating over the enum

const FLE_PTH: &str = "military.json";
const FLE_PTH_ADR: &str = "military_adr.json";

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Military {
    pub name: String,
    pub role: Role,
    pub persons: Vec<Person>,
}
impl Military {
    pub fn new() -> Self {
        Self {
            name: "U.S. Department of Defense".into(),
            role: Role::Military,
            persons: Vec::with_capacity(29),
        }
    }

    pub async fn load() -> Result<Military> {
        // Read members file from disk.

        let military = match read_from_file::<Military>(FLE_PTH) {
            Ok(military_from_disk) => military_from_disk,
            Err(_) => {
                let mut military = Military::new();

                let adrs = &fetch_adrs().await?;

                // Fetch members.
                military.fetch_members_dod().await?;
                military.fetch_members_oni(adrs).await?;
                military.fetch_members_usff(adrs).await?;

                // Write file to disk.
                write_to_file(&military, FLE_PTH)?;

                military
            }
        };

        println!("{} military leaders", military.persons.len());

        Ok(military)
    }

    pub async fn fetch_members_dod(&mut self) -> Result<()> {
        let url = "https://www.defense.gov/Contact/Mailing-Addresses/";
        let html = fetch_html(url).await?;
        let document = Html::parse_document(&html);

        let selector = Selector::parse("div.address-each").unwrap();
        for elm in document.select(&selector) {
            // Get lines and filter.
            let mut cur_lnes = elm
                .text()
                .map(|s| s.trim().to_string())
                .filter(|s| PRSR.filter(s))
                .collect::<Vec<String>>();
            eprintln!("{cur_lnes:?}");

            // Parse person.
            let mut per = Person {
                name: name_clean(&cur_lnes[0]),
                ..Default::default()
            };
            per.title1.clone_from(&cur_lnes[1].to_uppercase());
            // Clean up title.
            if let Some(idx) = per.title1.find('/') {
                per.title1.truncate(idx);
            } else if per.title1.contains(',') {
                per.title1 = per.title1.replace(',', " OF THE");
            }
            if let Some(idx) = per.title1.find("OF DEFENSE ") {
                per.title2 = per.title1[idx + 11..].trim().into();
                per.title1.truncate(idx + 11 - 1);
            }
            // Validate person.
            if per.name.is_empty() {
                return Err(anyhow!("name is empty {:?}", per));
            }
            if per.title1.is_empty() {
                return Err(anyhow!("title is empty {:?}", per));
            }

            // Parse address.
            let mut adr = Address::default();
            let mut lne = cur_lnes[2].clone();
            let lne_zip = &lne[lne.len() - 10..];
            let is_zip5 = is_zip5(lne_zip);
            let is_zip10 = if !is_zip5 { is_zip10(lne_zip) } else { false };
            if is_zip5 {
                adr.zip5 = lne_zip.parse().unwrap();
            } else {
                adr.zip5 = lne_zip[..5].parse().unwrap();
                adr.zip4 = lne_zip[lne_zip.len() - 4..].parse().unwrap();
            }
            adr.state = "DC".into();
            adr.city = "WASHINGTON".into();
            lne = lne[..lne.len() - 27].into();
            // Set Address2 if necessary.
            if lne.contains(" STE ") {
                if let Some(idx) = lne.find("STE") {
                    adr.address2 = Some(lne[idx..].into());
                    lne = lne[..idx - 2].trim().into();
                }
            }
            // Trim excess address if necessary.
            if let Some(idx_lne) = lne.rfind(',') {
                lne = lne[idx_lne + 1..].trim().into();
            }
            adr.address1.clone_from(&lne);

            // eprintln!("    {lne}");
            // eprintln!("  {adr:?}");

            let mut adrs = vec![adr];
            adrs = standardize_addresses(adrs).await?;

            per.adrs = Some(adrs);
            self.persons.push(per);
        }

        Ok(())
    }

    pub async fn fetch_members_oni(&mut self, adrs: &HashMap<Center, Address>) -> Result<()> {
        // Fetch url.
        let url = "https://www.oni.navy.mil/About/Biographies/";
        let html = fetch_html(url).await?;
        let document = Html::parse_document(&html);

        // Define the CSS selector for the members list.
        let hdr_sel = Selector::parse("h2").unwrap();
        let tbl_sel = Selector::parse("div.BioWrap").unwrap();
        let row_sel = Selector::parse("div.BioSenLead").unwrap();
        let name_sel = Selector::parse("p a").unwrap();

        for (idx, tbl_elm) in document.select(&tbl_sel).enumerate() {
            // eprintln!("  {}", hdrs[idx]);
            for row_elm in tbl_elm.select(&row_sel) {
                if let Some(elm) = row_elm.select(&name_sel).next() {
                    // Find name and office.
                    let full_name = elm.text().collect::<String>();
                    // eprintln!("{}", full_name.trim());

                    // Find name.
                    let mut full_name = full_name
                        .split_terminator('\n')
                        .next()
                        .unwrap_or_default()
                        .to_string();
                    if let Some(idx_fnd) = full_name.find(',') {
                        full_name.truncate(idx_fnd);
                    }

                    // Create person.
                    let mut per = Person {
                        name: name_clean(&full_name),
                        adrs: Some(vec![adrs[&Oni].clone()]),
                        ..Default::default()
                    };
                    if per.name.is_empty() {
                        return Err(anyhow!("name is empty"));
                    }

                    eprintln!("{}", per);
                    self.persons.push(per);
                }
            }
        }

        Ok(())
    }

    pub async fn fetch_members_usff(&mut self, adrs: &HashMap<Center, Address>) -> Result<()> {
        let urls = ["https://www.usff.navy.mil/Leadership/Biographies/Article/2375906/commander-usff/", "https://www.usff.navy.mil/Leadership/Biographies/Article/2728519/deputy-commander-usff/", "https://www.usff.navy.mil/Leadership/Biographies/Article/2728549/fleet-master-chief/"];

        for url in urls {
            // Fetch url.
            let html = fetch_html(url).await?;
            let document = Html::parse_document(&html);

            // Select name.
            let name_sel = Selector::parse("h1.maintitle").unwrap();
            if let Some(elm) = document.select(&name_sel).next() {
                let mut full_name = elm.text().collect::<String>();
                if full_name.contains("FLEET MASTER CHIEF") {
                    full_name = full_name.replace("FLEET MASTER CHIEF", "");
                    full_name = full_name.to_title_case();
                    full_name.insert_str(0, "FLTCM. ");
                }
                // eprintln!("{}", full_name.trim());

                let mut per = Person {
                    name: name_clean(&full_name),
                    adrs: Some(vec![adrs[&Usff].clone()]),
                    ..Default::default()
                };

                eprintln!("{}", per);
                self.persons.push(per);
            }
        }

        Ok(())
    }
}

/// Fetch, parse, and standardize an address.
pub async fn fetch_prs_std_adr(ctr: Center, url: &str) -> Result<Option<Address>> {
    // Fetch html.
    let html = fetch_html(url).await?;

    // Parse html to address lines.
    let adr_lnes_o = prs_adr_lnes(ctr, &html);

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
                    Some(adrs.remove(0))
                }
            }
        },
    };

    Ok(adrs_o)
}

pub fn prs_adr_lnes(ctr: Center, html: &str) -> Option<Vec<String>> {
    let document = Html::parse_document(html);
    let mut lnes: Vec<String> = Vec::new();
    for txt in ["h6", "span", "body"] {
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
    edit_mil_lnes(ctr, &mut lnes);
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

pub fn edit_mil_lnes(ctr: Center, lnes: &mut [String]) {
    if ctr == Oni {
        for idx in (0..lnes.len()).rev() {
            if let Some(idx_fnd) = lnes[idx].find(", USA") {
                lnes[idx].truncate(idx_fnd);
            }
        }
    }
}

pub async fn fetch_adrs() -> Result<HashMap<Center, Address>> {
    // Read file from disk.
    let mut map_adrs = match read_from_file::<HashMap<Center, Address>>(FLE_PTH_ADR) {
        Ok(map_adrs) => map_adrs,
        Err(_) => {
            let mut map_adrs = HashMap::new();

            // Iterate through each center.
            for ctr in Center::iter() {
                println!("{:?}", ctr);

                // Get url.
                let url = adr_url(ctr);
                if url.is_empty() {
                    continue;
                }

                // Fetch, parse, and standardize each address.
                match fetch_prs_std_adr(ctr, &url).await? {
                    None => {}
                    Some(mut adr) => {
                        map_adrs.insert(ctr, adr);
                    }
                }
            }

            // Write file to disk.
            write_to_file(&map_adrs, FLE_PTH_ADR)?;

            map_adrs
        }
    };

    Ok(map_adrs)
}

fn adr_url(ctr: Center) -> String {
    match ctr {
        Oni => "https://www.oni.navy.mil/Contact-Us/",
        Usff => "https://www.usa.gov/agencies/u-s-fleet-forces-command",
    }
    .into()
}

#[derive(
    Debug, EnumIter, Clone, Copy, Hash, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord,
)]
pub enum Center {
    Oni,  // Office of Naval Intelligence
    Usff, // U.S. Fleet Forces Command
}
