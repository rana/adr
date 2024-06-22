use crate::core::*;
use crate::models::*;
use crate::prsr::*;
use crate::usps::*;
use anyhow::{anyhow, Result};
use reqwest::Client;
use scraper::{Html, Selector};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt::Display;
use std::fs::File;
use std::io::{BufReader, BufWriter};
use std::ops::Add;
use std::path::Path;
use strum::EnumIter; // Required to derive EnumIter
use strum::IntoEnumIterator; // Required for iterating over the enum
use Center::*;

const FLE_PTH: &str = "nasa.json";
const FLE_PTH_ADR: &str = "nasa_adr.json";

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Nasa {
    pub name: String,
    pub role: Role,
    pub persons: Vec<Person>,
}

impl Nasa {
    pub fn new() -> Self {
        Self {
            name: "Scientific leaders".into(),
            role: Role::Scientific,
            persons: Vec::with_capacity(100),
        }
    }

    pub async fn load() -> Result<Nasa> {
        // Read file from disk.
        let mut nasa = match read_from_file::<Nasa>(FLE_PTH) {
            Ok(nasa_from_disk) => nasa_from_disk,
            Err(_) => {
                let mut nasa = Nasa::new();

                let adrs = &fetch_adrs().await?;

                // Fetch members.
                nasa.persons.extend(nasa.fetch_members_hq(adrs).await?);

                // Directorates
                nasa.persons.extend(nasa.fetch_members_armd(adrs).await?);
                nasa.persons.extend(nasa.fetch_members_esdmd(adrs).await?);
                nasa.persons.extend(nasa.fetch_members_stmd(adrs).await?);
                nasa.persons.extend(nasa.fetch_members_somd(adrs).await?);

                // Centers
                nasa.persons.extend(nasa.fetch_members_ames_1(adrs).await?);
                nasa.persons.extend(nasa.fetch_members_ames_2(adrs).await?);
                nasa.persons
                    .extend(nasa.fetch_members_ames_science_staff(adrs).await?);
                nasa.persons
                    .extend(nasa.fetch_members_armstrong(adrs).await?);
                nasa.persons.extend(nasa.fetch_members_glenn(adrs).await?);
                nasa.persons.extend(nasa.fetch_members_goddard(adrs).await?);
                nasa.persons.extend(nasa.fetch_members_johnson(adrs).await?);

                // nasa.persons.sort_unstable();
                nasa.persons.dedup_by(|a, b| a == b);

                // Write file to disk.
                write_to_file(&nasa, FLE_PTH)?;

                nasa
            }
        };

        println!("{} scientific leaders", nasa.persons.len());

        Ok(nasa)
    }

    pub async fn fetch_members_hq(&self, adrs: &HashMap<Center, Address>) -> Result<Vec<Person>> {
        let url = "https://www.nasa.gov/organization";
        let html = fetch_html(url).await?;
        let document = Html::parse_document(&html);

        // Define the CSS selector for the members list.
        let hdr_sel = Selector::parse("h1.wp-block-heading").unwrap();
        let tbl_sel = Selector::parse("table").unwrap();
        let row_sel = Selector::parse("tr").unwrap();
        let name_sel = Selector::parse("td:nth-of-type(1)").unwrap();
        let title_sel = Selector::parse("td:nth-of-type(2)").unwrap();
        let office_sel = Selector::parse("td:nth-of-type(3)").unwrap();

        // Select all headers.
        let hdrs = document
            .select(&hdr_sel)
            .map(|elm| elm.text().collect::<String>().to_uppercase())
            .collect::<Vec<_>>();
        // eprintln!("{hdrs:?}");

        // Iterate over each member entry.
        let mut pers = Vec::new();
        for (idx, tbl_elm) in document.select(&tbl_sel).enumerate() {
            if hdrs[idx] == "CENTERS AND FACILITIES" {
                continue;
            }
            eprintln!("  {}", hdrs[idx]);
            for row_elm in tbl_elm.select(&row_sel) {
                if let Some(elm) = row_elm.select(&name_sel).next() {
                    let full_name = elm.text().collect::<String>();
                    if full_name.trim().contains("(Vacant)") {
                        continue;
                    }
                    //eprintln!("{}", full_name.trim());
                    let mut per = Person {
                        name: name_clean(&full_name),
                        adrs: Some(vec![adrs[&HQ].clone()]),
                        ..Default::default()
                    };

                    eprintln!("{}", per);
                    pers.push(per);
                }
            }
        }

        Ok(pers)
    }

    pub async fn fetch_members_armd(&self, adrs: &HashMap<Center, Address>) -> Result<Vec<Person>> {
        let url = "https://www.nasa.gov/directorates/armd/aeronautics-leadership/";
        let html = fetch_html(url).await?;
        let document = Html::parse_document(&html);

        // Define the CSS selector for the members list.
        let hdr_sel = Selector::parse("h2.section-heading-sm").unwrap();
        let tbl_sel = Selector::parse("div.hds-card-grid").unwrap();
        let row_sel = Selector::parse("div.hds-card-inner").unwrap();
        let name_sel = Selector::parse("h3").unwrap();

        // Select all headers.
        let hdrs = document
            .select(&hdr_sel)
            .map(|elm| elm.text().collect::<String>().to_uppercase())
            .collect::<Vec<_>>();
        // eprintln!("{hdrs:?}");

        // Iterate over each member entry.
        let mut pers = Vec::new();
        for (idx, tbl_elm) in document.select(&tbl_sel).enumerate() {
            eprintln!("  {}", hdrs[idx]);
            for row_elm in tbl_elm.select(&row_sel) {
                if hdrs[idx] != "OFFICE OF THE ASSOCIATE ADMINISTRATOR" && hdrs[idx] != "OFFICES" {
                    continue;
                }

                if let Some(elm) = row_elm.select(&name_sel).next() {
                    let full_name = elm.text().collect::<String>();
                    //eprintln!("{}", full_name.trim());
                    let mut per = Person {
                        name: name_clean(&full_name),
                        adrs: Some(vec![adrs[&HQ].clone()]),
                        ..Default::default()
                    };

                    eprintln!("{}", per);
                    pers.push(per);
                }
            }
        }

        Ok(pers)
    }

    pub async fn fetch_members_esdmd(
        &self,
        adrs: &HashMap<Center, Address>,
    ) -> Result<Vec<Person>> {
        let url = "https://www.nasa.gov/exploration-systems-development-mission-directorate/";
        let html = fetch_html(url).await?;
        let document = Html::parse_document(&html);

        // Define the CSS selector for the members list.
        let hdr_sel = Selector::parse("h2.section-heading-sm").unwrap();
        let tbl_sel = Selector::parse("div.hds-card-grid").unwrap();
        let row_sel = Selector::parse("div.hds-card-inner").unwrap();
        let name_sel = Selector::parse("h3").unwrap();

        // Select all headers.
        let hdrs = document
            .select(&hdr_sel)
            .map(|elm| elm.text().collect::<String>().to_uppercase())
            .collect::<Vec<_>>();
        // eprintln!("{hdrs:?}");

        // Iterate over each member entry.
        let mut pers = Vec::new();
        for (idx, tbl_elm) in document.select(&tbl_sel).enumerate() {
            eprintln!("  {}", hdrs[idx]);
            for row_elm in tbl_elm.select(&row_sel) {
                if hdrs[idx] != "ESDMD LEADERSHIP" && hdrs[idx] != "MOON TO MARS PROGRAM OFFICE" {
                    continue;
                }

                if let Some(elm) = row_elm.select(&name_sel).next() {
                    let full_name = elm.text().collect::<String>();
                    //eprintln!("{}", full_name.trim());
                    let mut per = Person {
                        name: name_clean(&full_name),
                        adrs: Some(vec![adrs[&HQ].clone()]),
                        ..Default::default()
                    };

                    eprintln!("{}", per);
                    pers.push(per);
                }
            }
        }

        Ok(pers)
    }

    pub async fn fetch_members_stmd(&self, adrs: &HashMap<Center, Address>) -> Result<Vec<Person>> {
        let url = "https://www.nasa.gov/about-stmd/";
        let html = fetch_html(url).await?;
        let document = Html::parse_document(&html);

        // Define the CSS selector for the members list.
        let hdr_sel = Selector::parse("h2.section-heading-sm").unwrap();
        let tbl_sel = Selector::parse("div.hds-card-grid").unwrap();
        let row_sel = Selector::parse("div.hds-card-inner").unwrap();
        let name_sel = Selector::parse("h3").unwrap();

        // Select all headers.
        let hdrs = document
            .select(&hdr_sel)
            .map(|elm| elm.text().collect::<String>().to_uppercase())
            .collect::<Vec<_>>();
        // eprintln!("{hdrs:?}");

        // Iterate over each member entry.
        let mut pers = Vec::new();
        for (idx, tbl_elm) in document.select(&tbl_sel).enumerate() {
            eprintln!("  {}", hdrs[idx]);
            for row_elm in tbl_elm.select(&row_sel) {
                if let Some(elm) = row_elm.select(&name_sel).next() {
                    let full_name = elm.text().collect::<String>();
                    //eprintln!("{}", full_name.trim());
                    let mut per = Person {
                        name: name_clean(&full_name),
                        adrs: Some(vec![adrs[&HQ].clone()]),
                        ..Default::default()
                    };

                    eprintln!("{}", per);
                    pers.push(per);
                }
            }
        }

        Ok(pers)
    }

    pub async fn fetch_members_somd(&self, adrs: &HashMap<Center, Address>) -> Result<Vec<Person>> {
        let url = "https://www.nasa.gov/directorates/space-operations/";
        let html = fetch_html(url).await?;
        let document = Html::parse_document(&html);

        // Define the CSS selector for the members list.
        let hdr_sel = Selector::parse("h2.section-heading-sm").unwrap();
        let tbl_sel = Selector::parse("div.hds-card-grid").unwrap();
        let row_sel = Selector::parse("div.hds-card-inner").unwrap();
        let name_sel = Selector::parse("h3").unwrap();

        // Select all headers.
        let hdrs = document
            .select(&hdr_sel)
            .map(|elm| elm.text().collect::<String>().to_uppercase())
            .collect::<Vec<_>>();
        // eprintln!("{hdrs:?}");

        // Iterate over each member entry.
        let mut pers = Vec::new();
        for (idx, tbl_elm) in document.select(&tbl_sel).enumerate() {
            eprintln!("  {}", hdrs[idx]);
            for row_elm in tbl_elm.select(&row_sel) {
                if hdrs[idx] != "SPACE OPERATIONS LEADERSHIP" {
                    continue;
                }

                if let Some(elm) = row_elm.select(&name_sel).next() {
                    let full_name = elm.text().collect::<String>();
                    //eprintln!("{}", full_name.trim());
                    let mut per = Person {
                        name: name_clean(&full_name),
                        adrs: Some(vec![adrs[&HQ].clone()]),
                        ..Default::default()
                    };

                    eprintln!("{}", per);
                    pers.push(per);
                }
            }
        }

        Ok(pers)
    }

    pub async fn fetch_members_ames_1(
        &self,
        adrs: &HashMap<Center, Address>,
    ) -> Result<Vec<Person>> {
        let url = "https://www.nasa.gov/ames/ames-leadership-organizations/";
        let html = fetch_html(url).await?;
        let document = Html::parse_document(&html);

        // Define the CSS selector for the members list.
        let a_sel = Selector::parse("div.hds-meet-the-content a").unwrap();

        // Iterate over each member entry.
        let mut pers = Vec::new();
        for elm in document.select(&a_sel) {
            let full_name = elm.text().collect::<String>();
            if full_name.trim() == "Ames Research Center" {
                continue;
            }
            //eprintln!("{}", full_name.trim());
            let mut per = Person {
                name: name_clean(&full_name),
                ..Default::default()
            };
            per.adrs = Some(vec![adrs[&Ames].clone()]);

            eprintln!("{}", per);
            pers.push(per);
        }

        Ok(pers)
    }

    pub async fn fetch_members_ames_2(
        &self,
        adrs: &HashMap<Center, Address>,
    ) -> Result<Vec<Person>> {
        let url = "https://www.nasa.gov/ames/science/management-support/";
        let html = fetch_html(url).await?;
        let document = Html::parse_document(&html);

        // Define the CSS selector for the members list.
        let tbl_sel = Selector::parse("div.hds-card-custom").unwrap();
        let row_sel = Selector::parse("div.hds-card-inner").unwrap();
        let name_sel = Selector::parse("h3").unwrap();

        // Iterate over each member entry.
        let mut pers = Vec::new();
        for tbl_elm in document.select(&tbl_sel) {
            for row_elm in tbl_elm.select(&row_sel) {
                if let Some(elm) = row_elm.select(&name_sel).next() {
                    let full_name = elm.text().collect::<String>();
                    //eprintln!("{}", full_name.trim());
                    let mut per = Person {
                        name: name_clean(&full_name),
                        adrs: Some(vec![adrs[&Ames].clone()]),
                        ..Default::default()
                    };

                    eprintln!("{}", per);
                    pers.push(per);
                }
            }
        }

        Ok(pers)
    }

    pub async fn fetch_members_ames_science_staff(
        &self,
        adrs: &HashMap<Center, Address>,
    ) -> Result<Vec<Person>> {
        let mut pers = Vec::new();
        let urls = [
            "https://www.nasa.gov/ames/space-biosciences/bioengineering-branch/scb-staff/",
            "https://www.nasa.gov/ames/space-biosciences/flight-systems-implementation/scf-staff/",
            "https://www.nasa.gov/ames/space-biosciences/space-biosciences-research-branch-staff/",
            "https://www.nasa.gov/earth-science-at-ames/who-we-are/members-sg/",
            "https://www.nasa.gov/earth-science-at-ames/who-we-are/members-sge/",
            "https://www.nasa.gov/earth-science-at-ames/who-we-are/members-sgg/",
            "https://www.nasa.gov/earth-science-project-office-espo/",
            "https://www.nasa.gov/earth-science-at-ames/who-we-are/members-asp/",
            "https://www.nasa.gov/space-science-and-astrobiology-at-ames/who-we-are/members-sta/",
            "https://www.nasa.gov/space-science-and-astrobiology-at-ames/who-we-are/members-stt/",
            "https://www.nasa.gov/space-science-and-astrobiology-at-ames/who-we-are/members-stx/",
        ];
        for url in urls {
            let html = fetch_html(url).await?;
            let document = Html::parse_document(&html);

            // Define the CSS selector for the members list.
            let tbl_sel = Selector::parse("div.grid-container").unwrap();
            let row_sel = Selector::parse("div.grid-col-12").unwrap();
            let name_sel = Selector::parse("h2").unwrap();

            // Iterate over each member entry.
            for tbl_elm in document.select(&tbl_sel) {
                for row_elm in tbl_elm.select(&row_sel) {
                    if let Some(elm) = row_elm.select(&name_sel).next() {
                        let full_name = elm.text().collect::<String>();
                        //eprintln!("{}", full_name.trim());
                        let mut per = Person {
                            name: name_clean(&full_name),
                            adrs: Some(vec![adrs[&Ames].clone()]),
                            ..Default::default()
                        };

                        eprintln!("{}", per);
                        pers.push(per);
                    }
                }
            }
        }

        Ok(pers)
    }

    pub async fn fetch_members_armstrong(
        &self,
        adrs: &HashMap<Center, Address>,
    ) -> Result<Vec<Person>> {
        let url = "https://www.nasa.gov/armstrong/people/leadership-organizations/#center-director";
        let html = fetch_html(url).await?;
        let document = Html::parse_document(&html);

        // Define the CSS selector for the members list.
        let tbl_sel = Selector::parse("p").unwrap();
        let title_sel = Selector::parse("strong").unwrap();
        let name_sel = Selector::parse("a").unwrap();

        // Iterate over each member entry.
        let mut pers = Vec::new();
        for tbl_elm in document.select(&tbl_sel) {
            if let Some(elm) = tbl_elm.select(&title_sel).next() {
                let title = elm.text().collect::<String>();
                // eprintln!("title:{title}");
                if title.trim().ends_with(':') {
                    if let Some(elm) = tbl_elm.select(&name_sel).next() {
                        let full_name = elm.text().collect::<String>();
                        //eprintln!("{}", full_name.trim());
                        let mut per = Person {
                            name: name_clean(&full_name),
                            adrs: Some(vec![adrs[&Armstrong].clone()]),
                            ..Default::default()
                        };
                        eprintln!("{}", per);
                        pers.push(per);
                    }
                }
            }
        }

        Ok(pers)
    }

    pub async fn fetch_members_glenn(
        &self,
        adrs: &HashMap<Center, Address>,
    ) -> Result<Vec<Person>> {
        let url = "https://www.nasa.gov/about-glenn-research-center/nasa-glenn-leadership/";
        let html = fetch_html(url).await?;
        let document = Html::parse_document(&html);

        // Define the CSS selector for the members list.
        let tbl_sel = Selector::parse("div.hds-card-custom").unwrap();
        let row_sel = Selector::parse("div.hds-card-inner").unwrap();
        let name_sel = Selector::parse("h3").unwrap();

        // Iterate over each member entry.
        let mut pers = Vec::new();
        for tbl_elm in document.select(&tbl_sel) {
            for row_elm in tbl_elm.select(&row_sel) {
                if let Some(elm) = row_elm.select(&name_sel).next() {
                    let full_name = elm.text().collect::<String>();
                    //eprintln!("{}", full_name.trim());
                    let mut per = Person {
                        name: name_clean(&full_name),
                        adrs: Some(vec![adrs[&Glenn].clone()]),
                        ..Default::default()
                    };

                    eprintln!("{}", per);
                    pers.push(per);
                }
            }
        }

        Ok(pers)
    }

    pub async fn fetch_members_goddard(
        &self,
        adrs: &HashMap<Center, Address>,
    ) -> Result<Vec<Person>> {
        let url = "https://www.nasa.gov/goddard/about/#leadership";
        let html = fetch_html(url).await?;
        let document = Html::parse_document(&html);

        // Define the CSS selector for the members list.
        let a_sel = Selector::parse("div.hds-meet-the-content a").unwrap();

        // Iterate over each member entry.
        let mut pers = Vec::new();
        for elm in document.select(&a_sel) {
            let full_name = elm.text().collect::<String>();
            //eprintln!("{}", full_name.trim());
            let mut per = Person {
                name: name_clean(&full_name),
                adrs: Some(vec![adrs[&Goddard].clone()]),
                ..Default::default()
            };

            eprintln!("{}", per);
            pers.push(per);
        }

        Ok(pers)
    }

    pub async fn fetch_members_johnson(
        &self,
        adrs: &HashMap<Center, Address>,
    ) -> Result<Vec<Person>> {
        let url = "https://www.nasa.gov/johnson/#leadership";
        let html = fetch_html(url).await?;
        let document = Html::parse_document(&html);

        // Define the CSS selector for the members list.
        let tbl_sel = Selector::parse("div.hds-card-grid").unwrap();
        let hdr_sel = Selector::parse("h2.section-heading-sm").unwrap();
        let row_sel = Selector::parse("div.hds-card-inner").unwrap();
        let name_sel = Selector::parse("h3").unwrap();

        // Iterate over each member entry.
        let mut pers = Vec::new();
        for tbl_elm in document.select(&tbl_sel) {
            // Select current header.
            if let Some(hdr_elm) = tbl_elm.select(&hdr_sel).next() {
                let hdr = hdr_elm.text().collect::<String>().to_uppercase();
                eprintln!("{hdr:?}");
                if &hdr != "JOHNSON LEADERSHIP" {
                    continue;
                }
            }
            // Select leaders.
            for row_elm in tbl_elm.select(&row_sel) {
                if let Some(elm) = row_elm.select(&name_sel).next() {
                    let full_name = elm.text().collect::<String>();
                    //eprintln!("{}", full_name.trim());
                    let full_name = full_name.split_terminator(',').next().unwrap_or_default();
                    let mut per = Person {
                        name: name_clean(full_name),
                        adrs: Some(vec![adrs[&Johnson].clone()]),
                        ..Default::default()
                    };

                    eprintln!("{}", per);
                    pers.push(per);
                }
            }
        }

        Ok(pers)
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
                        // Easy way to clean address2. Due to "CENTER".
                        adr.address2 = None;
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
    for txt in ["body"] {
        let selector = Selector::parse(txt).unwrap();
        for elm in document.select(&selector) {
            // Extract lines from html.
            let mut cur_lnes = elm
                .text()
                .map(|s| s.trim().trim_end_matches(',').to_uppercase().to_string())
                .collect::<Vec<String>>();

            // eprintln!("--- pre: {cur_lnes:?}");

            // Filter lines.
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
    edit_nbsp_zwsp(&mut lnes);
    edit_mailing(&mut lnes);
    edit_nasa_lnes(ctr, &mut lnes);
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

pub fn edit_nasa_lnes(ctr: Center, lnes: &mut Vec<String>) {
    match ctr {
        HQ => {
            for idx in (0..lnes.len()).rev() {
                if lnes[idx] == "300 E STREET SW, SUITE 5R30" {
                    lnes[idx] = "300 E STREET SW".into();
                }
            }
        }
        Goddard => {
            for idx in (0..lnes.len()).rev() {
                if lnes[idx] == "9432 GREENBELT ROAD" {
                    lnes.remove(idx + 1);
                    lnes.remove(idx);
                }
            }
        }
        Kennedy => {
            for idx in (0..lnes.len()).rev() {
                if lnes[idx] == "JOHN F KENNEDY SPACE CENTER" {
                    lnes[idx] = "KENNEDY SPACE CENTER".into();
                }
            }
        }
        Jpl => {
            for idx in (0..lnes.len()).rev() {
                if lnes[idx].starts_with("STREET ADDRESS FOR USE") {
                    lnes.remove(idx + 2);
                    lnes.remove(idx + 1);
                }
            }
        }
        Marshall => {
            for idx in (0..lnes.len()).rev() {
                if lnes[idx].starts_with("PO BOX") {
                    lnes[idx] = "MARSHALL SPACE FLIGHT CENTER".into();
                }
            }
        }
        Langley => {
            for idx in (0..lnes.len()).rev() {
                if lnes[idx].contains("23681-2199") {
                    lnes[idx] = lnes[idx].replace("23681-2199", "23681")
                }
            }
        }
        _ => {}
    }
}

fn adr_url(ctr: Center) -> String {
    match ctr {
        Ames => "https://www.nasa.gov/ames-earth-science-contact-us/",
        Armstrong => "https://www.nasa.gov/armstrong/overview/",
        Glenn => "https://www.grc.nasa.gov/WWW/K-12/directions.html",
        Goddard => "https://www.nasa.gov/centers-and-facilities/goddard/driving-directions-to-the-goddard-visitor-center/",
        HQ => "https://www.nasa.gov/contact/",
        Johnson => "https://www.nasa.gov/johnson/center-operations-directorate/",
        Jpl => "https://www.jpl.nasa.gov/jpl-and-the-community/directions-and-maps",
        Kennedy => "https://www.nasa.gov/kennedy-information/",
        Langley => "https://www.nasa.gov/centers-and-facilities/langley/contacting-nasas-langley-research-center/",
        Marshall => "https://www.nasa.gov/marshall/visit-marshall-space-flight-center/",
        Safety => "https://www.nasa.gov/nasa-safety-center-overview/#contact",
    }
    .into()
}

#[derive(
    Debug, EnumIter, Clone, Copy, Hash, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord,
)]
pub enum Center {
    Ames,
    Armstrong,
    Glenn,
    Goddard, // Goddard Space Flight Center
    HQ,      // Headquarters
    Johnson,
    Jpl, // Jet Propulsion Laboratory
    Kennedy,
    Langley,
    Marshall,
    Safety, // Safety Center
}
