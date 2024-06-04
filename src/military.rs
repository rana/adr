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
            Ok(house_from_disk) => {
                // Read from disk.
                house_from_disk
            }
            Err(err) => {
                // File not available.
                eprintln!("read file {}: err: {}", FLE_PTH, err);

                let mut military = Military::new();

                // Fetch list of people from network.
                // URL of the US Department of Defense page.
                const URL: &str = "https://www.defense.gov/Contact/Mailing-Addresses/";

                // Fetch the members directory webpage.
                let cli = Client::new();
                let html = fetch_html(URL, &cli).await?;

                // Extract members from html.
                military.extract_members(&html);

                // Validate members fields.
                military.validate_members()?;

                // Standardize addresses.
                military.standardize_addresses().await?;

                // eprintln!("{:?}", military);

                // Write members to disk.
                write_to_file(&military, FLE_PTH)?;

                military
            }
        };

        println!("{} military leaders", military.persons.len());

        Ok(military)
    }

    /// Extract members from the specified html.
    pub fn extract_members(&mut self, html: &str) {
        let prsr = &Prsr::new();
        let document = Html::parse_document(html);
        let selector = Selector::parse("div.address-each").unwrap();
        for elm in document.select(&selector) {
            // Get lines and filter.
            let mut cur_lnes = elm
                .text()
                .map(|s| s.trim().to_string())
                .filter(|s| prsr.filter(s))
                .collect::<Vec<String>>();
            eprintln!("{cur_lnes:?}");

            // Parse person.
            let mut per = Person::default();
            per.name_fst.clone_from(&cur_lnes[0]);
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

            per.adrs = Some(vec![adr]);
            self.persons.push(per);
        }
    }

    pub async fn standardize_addresses(&mut self) -> Result<()> {
        let mut cli = &Client::new();
        for per in self.persons.iter_mut() {
            let mut adrs = per.adrs.as_mut().unwrap();
            standardize_addresses(adrs, cli).await?;
            eprintln!("  {}", adrs[0]);
        }

        Ok(())
    }

    pub fn validate_members(&mut self) -> Result<()> {
        for per in &self.persons {
            if per.name_fst.is_empty() {
                return Err(anyhow!("person: name_fst empty {:?}", per));
            }
            if per.title1.is_empty() {
                return Err(anyhow!("person: title empty {:?}", per));
            }
            validate_addresses(per, per.adrs.as_deref().unwrap())?;
        }

        Ok(())
    }
}
