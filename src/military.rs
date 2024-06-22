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

const FLE_PTH: &str = "military.json";

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
            persons: Vec::with_capacity(17),
        }
    }

    pub async fn load() -> Result<Military> {
        // Read members file from disk.

        let military = match read_from_file::<Military>(FLE_PTH) {
            Ok(military_from_disk) => military_from_disk,
            Err(_) => {
                let mut military = Military::new();

                // Fetch members.
                military.fetch_members().await?;

                military
            }
        };

        println!("{} military leaders", military.persons.len());

        Ok(military)
    }

    /// Fetch members from network.
    pub async fn fetch_members(&mut self) -> Result<()> {
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
            adr.zip = lne[lne.len() - 10..].into();
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

            // Checkpoint save.
            // Write intermediate file to disk.
            write_to_file(&self, FLE_PTH)?;
        }

        Ok(())
    }
}
