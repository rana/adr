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

                // Fetch list of representatives from network.
                // URL of the House of Representatives page.
                const URL: &str = "https://www.house.gov/representatives";

                // Fetch the representatives directory webpage.
                let cli = Client::new();
                let html = fetch_html(URL, &cli).await?;

                // Extract representatives from html.
                house.extract_members(&html);

                // Validate representative fields.
                house.validate_members()?;

                // Write representatives to disk.
                write_to_file(&house, FLE_PTH)?;

                house
            }
        };

        println!("{} representatives", house.persons.len());

        Ok(house)
    }

    /// Extract house members from the specified html.
    pub fn extract_members(&mut self, html: &str) {
        let document = Html::parse_document(html);

        // Define the CSS selector for the representatives list
        let selector = Selector::parse("table.table tr").unwrap();
        let name_selector = Selector::parse("td:nth-of-type(1)").unwrap();
        let url_selector = Selector::parse("td:nth-of-type(1) a").unwrap();

        // Iterate over each representative entry
        for element in document.select(&selector) {
            if let Some(name_element) = element.select(&name_selector).next() {
                let mut rep = Person::default();
                if let Some((name_lst, name_fst)) = name_element
                    .text()
                    .collect::<Vec<_>>()
                    .join(" ")
                    .split_once(',')
                {
                    rep.name_fst = name_fst.trim().to_string();
                    rep.name_lst = name_lst.trim().to_string();
                }
                // Skip empty or vacancy.
                // "Mike - Vacancy"
                if rep.name_fst.is_empty() || rep.name_fst.ends_with("Vacancy") {
                    continue;
                }
                rep.url = element
                    .select(&url_selector)
                    .next()
                    .map_or(String::new(), |a| {
                        a.value()
                            .attr("href")
                            .unwrap_or("")
                            .trim_end_matches('/')
                            .to_string()
                    });

                self.persons.push(rep);
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
        }
        Ok(())
    }

    pub async fn fetch_addresses(&mut self) -> Result<()> {
        let cli = &Client::new();
        let prsr = &Prsr::new();

        // Clone self before iterating over self.persons to avoid borrowing conflicts.
        // For checkpoint saving.
        let mut self_clone = self.clone();

        for (idx, per) in self
            .persons
            .iter_mut()
            .enumerate()
            .filter(|(_, per)| per.adrs.is_none())
            .take(1)
        {
            eprintln!("  {} {} {} {}", idx, per.name_fst, per.name_lst, per.url);

            // Fetch addresses into person.
            let mut has_adrs =
                fetch_addresses(&mut self_clone, idx, per, "contact", cli, prsr).await?;
            if !has_adrs {
                has_adrs = fetch_addresses(&mut self_clone, idx, per, "contact/offices", cli, prsr)
                    .await?;
                if !has_adrs {
                    has_adrs = fetch_addresses(
                        &mut self_clone,
                        idx,
                        per,
                        "contact/office-locations",
                        cli,
                        prsr,
                    )
                    .await?;
                    if !has_adrs {
                        fetch_addresses(&mut self_clone, idx, per, "offices", cli, prsr).await?;
                    }
                }
            }
        }

        Ok(())
    }
}

pub async fn fetch_addresses(
    self_clone: &mut House,
    idx: usize,
    per: &mut Person,
    url_path: &str,
    cli: &Client,
    prsr: &Prsr,
) -> Result<bool> {
    let mut adr_lnes_o = fetch_address_lnes(per, url_path, cli, prsr).await?;

    // Parse lines to Addresses.
    match adr_lnes_o {
        None => return Ok(false),
        Some(adr_lnes) => {
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

pub async fn fetch_address_lnes(
    per: &mut Person,
    url_path: &str,
    cli: &Client,
    prsr: &Prsr,
) -> Result<Option<Vec<String>>> {
    // Some representative addresses are in a contact webpage.

    // Fetch a URL.
    let url = format!("{}/{}", per.url, url_path);
    let html = fetch_html(url.as_str(), cli).await?;

    // Parse HTML.
    let document = Html::parse_document(&html);

    // Attempt to select addresses from various sections of the HTML.
    let mut lnes: Vec<String> = Vec::new();
    for txt in [
        "address",
        ".internal__offices--address",
        ".office-locations",
        "body",
    ] {
        let selector = Selector::parse(txt).unwrap();
        for elm in document.select(&selector) {
            // Get lines and filter.
            let cur_lnes = elm
                .text()
                .map(|s| s.trim().trim_end_matches(',').to_uppercase().to_string())
                .filter(|s| prsr.filter(s))
                .collect::<Vec<String>>();

            eprintln!("{cur_lnes:?}");

            lnes.extend(cur_lnes);
        }

        if !lnes.is_empty() {
            break;
        }
    }

    // eprintln!("{lnes:?}");

    // Edit lines to make it easier to parse.
    prsr.edit_lnes(&mut lnes);
    edit_hob(&mut lnes);

    // eprintln!("--- {lnes:?}");

    if has_lne_zip(&lnes) {
        return Ok(Some(lnes));
    }

    Ok(None)
}
