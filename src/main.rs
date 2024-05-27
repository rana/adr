use anyhow::{anyhow, Result};
use csv::Writer;
use reqwest::Client;
use scraper::{Html, Selector};
use serde::{Deserialize, Serialize};
use std::fs::File;
use std::io::{BufReader, BufWriter};
use std::path::Path;
mod prsr;
mod usps;
use prsr::Prsr;
use usps::UspsClient;
mod models;
use models::Address;

#[tokio::main]
pub async fn main() -> Result<()> {
    let reps = load_house_list().await?;
    println!("{} representatives", reps.len());
    let mut usps_cli = UspsClient::new();
    let prsr = Prsr::new();
    let adrs = load_house_addresses(&reps, 297, usize::MAX, &mut usps_cli, &prsr).await?;
    println!("{} representative addresses", adrs.len());
    write_addresses_to_csv("house.csv", &adrs)?;
    Ok(())
}

pub async fn load_house_list() -> Result<Vec<House>> {
    // File path to store the JSON data.
    let file_path = "house_list.json";
    // Read representatives file from disk.
    let reps = match read_from_file::<House>(file_path) {
        Ok(reps_from_disk) => {
            // Read from disk.
            reps_from_disk
        }
        Err(err) => {
            // File not available.
            eprintln!("read file {}: err: {}", file_path, err);

            // Fetch list from network.
            // URL of the House of Representatives page.
            let url = "https://www.house.gov/representatives";

            // Fetch and parse the directory webpage.
            let cli = Client::new();
            let html = fetch_html(url, &cli).await?;

            // Extract representatives from html.
            let reps_from_net = extract_houses(&html)?;

            // Validate fields.
            validate_houses(&reps_from_net)?;

            // Write representatives to disk.
            write_to_file(&reps_from_net, file_path)?;

            reps_from_net
        }
    };

    Ok(reps)
}

pub async fn load_house_addresses(
    reps: &[House],
    idx_skp: usize,
    cnt_tak: usize,
    usps_cli: &mut UspsClient,
    prsr: &Prsr,
) -> Result<Vec<Address>> {
    // File path to store the JSON data.
    let file_path = "house_addresses.json";

    // TODO: UNCOMMENT
    //// Read representatives file from disk.
    // let adrs = match read_from_file::<Address>(file_path) {
    //     Ok(adrs_from_disk) => {
    //         // Read from disk.
    //         adrs_from_disk
    //     }
    //     Err(err) => {
    //         // File not available.
    //         eprintln!("read file {}: err: {}", file_path, err);

    // Fetch from network.
    let mut adrs: Vec<Address> = Vec::with_capacity(reps.len() * 4);
    let cli = Client::new();
    for (idx, rep) in reps.iter().enumerate().skip(idx_skp).take(cnt_tak) {
        eprint!("idx:{} ", idx);

        // Fetch representative addresses as Vec<String>.
        let mut adr_lnes = fetch_house_addresses_contact(rep, "contact", &cli, prsr).await?;
        if adr_lnes.is_none() {
            adr_lnes = fetch_house_addresses_contact(rep, "contact/offices", &cli, prsr).await?;
            if adr_lnes.is_none() {
                adr_lnes =
                    fetch_house_addresses_contact(rep, "contact/office-locations", &cli, prsr)
                        .await?;
                if adr_lnes.is_none() {
                    adr_lnes = fetch_house_addresses_main_footer(rep, &cli, prsr).await?;
                    if adr_lnes.is_none() {
                        adr_lnes = fetch_house_addresses_main(rep, &cli, prsr).await?;
                    }
                }
            }
        }
        // if idx_skp != 0 {
        //     eprintln!("{:?}", adr_lnes);
        // }

        match adr_lnes {
            None => {
                return Err(anyhow!(
                    "representative: missing address lines (idx:{} rep:{:?})",
                    idx,
                    rep
                ));
            }
            Some(lnes) => {
                // Parse addresses for current representative.
                match extract_house_addresses(rep, &lnes) {
                    None => {
                        return Err(anyhow!("house: no addresses parsed for {:?}", rep));
                    }
                    Some(mut new_adrs) => {
                        // Validate addresses for current representative.
                        validate_house_addresses(rep, &new_adrs)?;

                        eprintln!("{new_adrs:?}");

                        // TODO:
                        // standardize_addresses(&mut new_adrs, usps_cli).await?;

                        // eprintln!("{new_adrs:?}");

                        adrs.extend(new_adrs);
                    }
                }
            }
        }
    }

    // Write addresses to disk.
    write_to_file(&adrs, file_path)?;

    // TODO: UNCOMMENT
    // adrs
    //     }
    // };

    Ok(adrs)
}

/// Selects representative addresses from the html footer.
pub async fn fetch_house_addresses_main_footer(
    rep: &House,
    cli: &Client,
    prsr: &Prsr,
) -> Result<Option<Vec<String>>> {
    // Fetch a URL.
    let html = fetch_html(&rep.url, cli).await?;

    // Some representative addresses are in the footer.
    // Parse HTML.
    let document = Html::parse_document(&html);

    // Define the CSS selector for the footer which contains mailing addresses.
    let selector = Selector::parse("footer").unwrap();
    if let Some(elm) = document.select(&selector).next() {
        // Get lines and filter.
        let mut lnes = elm
            .text()
            .map(|s| s.trim().trim_end_matches(',').to_uppercase().to_string())
            .filter(|s| {
                !s.is_empty()
                    && !s.contains("DOCUMENT.QUERYSELECTOR")
                    && !s.contains("FORM")
                    && !s.contains("IFRAME")
                    && !s.contains("FUNCTION")
                    && !s.contains("Z-INDEX")
                    && !s.contains("!IMPORTANT;")
                    && !s.starts_with('(')
                    && !s.contains("PHONE:")
                    && !s.contains("FAX:")
                    && !s.contains("A.M.")
                    && !prsr.re_flt.is_match(s)
            })
            .collect::<Vec<String>>();

        eprintln!("{lnes:?}");

        // Edit the footer text to make it easier to parse.
        edit_split_bar(&mut lnes);
        edit_split_city_state_zip(&mut lnes);
        edit_drain_after_last_zip(&mut lnes);
        edit_hob(&mut lnes);
        edit_dc(&mut lnes);

        // edit_split_suite(&mut lnes);

        eprintln!("{lnes:?}");

        if has_lne_zip(&lnes) {
            return Ok(Some(lnes));
        }
    }

    Ok(None)
}

pub async fn fetch_house_addresses_main(
    rep: &House,
    cli: &Client,
    prsr: &Prsr,
) -> Result<Option<Vec<String>>> {
    // Fetch a URL.
    let html = fetch_html(&rep.url, cli).await?;

    // Some representative addresses are in the footer.
    // Parse HTML.
    let document = Html::parse_document(&html);

    // Define the CSS selector for the footer which contains mailing addresses.
    let selector = Selector::parse("body").unwrap();
    if let Some(elm) = document.select(&selector).next() {
        // Get lines and filter.
        let mut lnes = elm
            .text()
            .map(|s| s.trim().trim_end_matches(',').to_uppercase().to_string())
            .filter(|s| {
                !s.is_empty()
                    && !s.contains("DOCUMENT.QUERYSELECTOR")
                    && !s.contains("FORM")
                    && !s.contains("IFRAME")
                    && !s.contains("FUNCTION")
                    && !s.contains("Z-INDEX")
                    && !s.contains("!IMPORTANT;")
                    && !s.starts_with('(')
                    && !s.contains("PHONE:")
                    && !s.contains("FAX:")
                    && !s.contains("A.M.")
                    && !prsr.re_flt.is_match(s)
            })
            .collect::<Vec<String>>();

        eprintln!("{lnes:?}");

        // Edit the footer text to make it easier to parse.
        edit_split_bar(&mut lnes);
        edit_split_city_state_zip(&mut lnes);
        edit_drain_after_last_zip(&mut lnes);
        edit_hob(&mut lnes);
        edit_dc(&mut lnes);

        // edit_split_suite(&mut lnes);

        eprintln!("{lnes:?}");

        if has_lne_zip(&lnes) {
            return Ok(Some(lnes));
        }
    }

    Ok(None)
}

pub async fn fetch_house_addresses_contact(
    rep: &House,
    path: &str,
    cli: &Client,
    prsr: &Prsr,
) -> Result<Option<Vec<String>>> {
    // Some representative addresses are in a contact webpage.

    // Fetch a URL.
    let url = format!("{}/{}", rep.url, path);
    let html = fetch_html(url.as_str(), cli).await?;

    // Parse HTML.
    let document = Html::parse_document(&html);

    // Define the CSS selector for the footer which contains mailing addresses.
    // let selector = Selector::parse(".contact-block").unwrap();
    // let selector = Selector::parse(".internal__offices--address").unwrap();
    // let selector = Selector::parse("address").unwrap();
    let mut lnes: Vec<String> = Vec::new();
    for txt in [
        "address",
        ".internal__offices--address",
        ".office-locations",
        "body",
    ] {
        let selector = Selector::parse(txt).unwrap();
        for elm in document.select(&selector) {
            // Fetching "https://allen.house.gov/contact"...
            // ["462 Cannon House Office Building", "\nWashington, DC 20515", "\nPhone: ", "(202) 225-2823", "\nFax: (202) 225-3377"]
            // ["2743 Perimeter Parkway", "\nBuilding 200, Suite 105", "\nAugusta, GA 30909", "\nPhone: ", "(706) 228-1980", "\nFax: (706) 228-1954"]
            // ["100 S. Church Street", "\nDublin, GA 31021", "\nPhone: ", "(478) 291-6324", "\nFax: (706) 228-1954"]
            // ["50 E. Main Street", "\nStatesboro, GA 30458", "\nPhone: ", "(912) 243-9452", "\nFax: (912) 243-9453"]
            // ["107 Old Airport Rd, Suite A", "\nVidalia, GA 304", "74", "\nPhone: ", "(912) 243-9452", "\nFax: (912) 243-9453"]

            // Get lines and filter.
            let cur_lnes = elm
                .text()
                .map(|s| s.trim().to_uppercase().to_string())
                .filter(|s| {
                    !s.is_empty()
                        && !s.contains("IFRAME")
                        && !s.contains("FUNCTION")
                        && !s.contains("FORM")
                        && !s.contains("!IMPORTANT;")
                        && !s.starts_with('(')
                        && !s.contains("PHONE:")
                        && !s.contains("FAX:")
                        && !prsr.re_flt.is_match(s)
                })
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
    edit_split_bar(&mut lnes);
    edit_split_city_state_zip(&mut lnes);
    edit_disjoint_zip(&mut lnes);
    edit_drain_after_last_zip(&mut lnes);
    edit_hob(&mut lnes);
    edit_dc(&mut lnes);

    // edit_split_suite(&mut lnes);

    // eprintln!("--- {lnes:?}");

    if has_lne_zip(&lnes) {
        return Ok(Some(lnes));
    }

    Ok(None)
}

pub fn validate_houses(reps: &[House]) -> Result<()> {
    for (idx, rep) in reps.iter().enumerate() {
        if rep.first_name.is_empty() {
            return Err(anyhow!(
                "representative: first name empty (idx:{} rep:{:?})",
                idx,
                rep
            ));
        }
        if rep.last_name.is_empty() {
            return Err(anyhow!(
                "representative: last name empty (idx:{} rep:{:?})",
                idx,
                rep
            ));
        }
        if rep.url.is_empty() {
            return Err(anyhow!(
                "representative: url empty (idx:{} rep:{:?})",
                idx,
                rep
            ));
        }
    }
    Ok(())
}

pub fn extract_houses(html: &str) -> Result<Vec<House>> {
    let mut reps = Vec::new();

    let document = Html::parse_document(html);

    // Define the CSS selector for the representatives list
    let selector = Selector::parse("table.table tr").unwrap();
    let name_selector = Selector::parse("td:nth-of-type(1)").unwrap();
    let url_selector = Selector::parse("td:nth-of-type(1) a").unwrap();

    // Iterate over each representative entry
    for element in document.select(&selector) {
        if let Some(name_element) = element.select(&name_selector).next() {
            let mut rep = House::default();
            if let Some((last_name, first_name)) = name_element
                .text()
                .collect::<Vec<_>>()
                .join(" ")
                .split_once(',')
            {
                rep.first_name = first_name.trim().to_string();
                rep.last_name = last_name.trim().to_string();
            }
            // Skip empty or vacancy.
            // "Mike - Vacancy"
            if rep.first_name.is_empty() || rep.first_name.ends_with("Vacancy") {
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
            reps.push(rep);
        }
    }

    Ok(reps)
}

pub async fn fetch_html(url: &str, cli: &Client) -> Result<String> {
    eprintln!("Fetching {url:?}...");
    let res = cli.get(url).send().await?;
    let bdy = res.text().await?;
    Ok(bdy)
}

// Function to serialize and write a list to a file in JSON format
pub fn write_to_file<T: Serialize>(data: &Vec<T>, file_path: &str) -> Result<()> {
    eprintln!("Writing file: {}", file_path);
    let file = File::create(file_path)?;
    let writer = BufWriter::new(file);
    serde_json::to_writer(writer, &data)?;
    Ok(())
}

// Function to deserialize and read a list from a file
pub fn read_from_file<T: for<'de> Deserialize<'de>>(file_path: &str) -> Result<Vec<T>> {
    eprintln!("Reading file: {}", file_path);
    let file = File::open(file_path)?;
    let reader = BufReader::new(file);
    let data = serde_json::from_reader(reader)?;
    Ok(data)
}

pub fn ends_with_5digits(lne: &str) -> bool {
    if lne.len() < 5 {
        return false;
    }
    lne.chars().skip(lne.len() - 5).all(|c| c.is_ascii_digit())
}

pub fn is_5digits(lne: &str) -> bool {
    lne.len() == 5 && ends_with_5digits(lne)
}

pub fn is_all_digits(lne: &str) -> bool {
    lne.chars().all(|c| c.is_ascii_digit())
}

pub fn has_lne_zip(lnes: &[String]) -> bool {
    let mut has_zip = false;

    for lne in lnes {
        if is_5digits(lne) {
            has_zip = true;
            break;
        }
    }

    has_zip
}

pub fn edit_drain_after_last_zip(lnes: &mut Vec<String>) {
    // Trim the list after the last zip code.
    // Search for the last zip code.
    for idx in (0..lnes.len()).rev() {
        if ends_with_5digits(&lnes[idx]) {
            lnes.drain(idx + 1..);
            break;
        }
    }
}

pub fn edit_hob(lnes: &mut Vec<String>) {
    // Trim list prefix prior to "House Office Building"
    // Reverse indexes to allow for room line removal.
    for idx in (0..lnes.len()).rev() {
        // "1107 LONGWORTH HOUSE", "OFFICE BUILDING"
        if idx + 1 != lnes.len()
            && lnes[idx].ends_with("HOUSE")
            && lnes[idx + 1] == "OFFICE BUILDING"
        {
            lnes[idx].push_str(" OFFICE BUILDING");
            lnes.remove(idx + 1);
        }

        // "2312 RAYBURN HOUSE OFFICE BUILDING"
        // "2430 RAYBURN HOUSE OFFICE BLDG."
        if let Some(hob_idx) = lnes[idx].find("HOUSE OFFICE") {
            lnes[idx].truncate(hob_idx);
            lnes[idx].push_str("HOB");
            // No break. Can have duplicate addresses.
        }
        // "1119 LONGWORTH H.O.B."
        // "H.O.B." -> "HOB"
        if let Some(hob_idx) = lnes[idx].find("H.O.B.") {
            lnes[idx].truncate(hob_idx);
            lnes[idx].push_str("HOB");
            // No break. Can have duplicate addresses.
        }
        // Insert Room number to HOB if necessary.
        // "LONGWORTH HOB", "ROOM 1027"
        // Still check for ends with HOB as some addresses may originally have it.
        if idx + 1 != lnes.len()
            && lnes[idx + 1].contains("ROOM")
            && lnes[idx].trim().ends_with("HOB")
        {
            let room: Vec<&str> = lnes[idx + 1].split_whitespace().collect();
            lnes[idx] = format!("{} {}", room[1], lnes[idx]);
            lnes.remove(idx + 1);
        }
    }
}

pub fn edit_split_suite(lnes: &mut Vec<String>) {
    // "29 Crafts Street, Suite 375"
    for idx in (0..lnes.len()).rev() {
        let lne_upr = lnes[idx].to_uppercase();
        if lne_upr.contains("SUITE") && lne_upr.contains(',') {
            let lne = lnes[idx].clone();
            lnes.remove(idx);
            for s in lne.split_terminator(',').rev() {
                lnes.insert(idx, s.to_string());
            }
        }
    }
}

pub fn edit_split_city_state_zip(lnes: &mut Vec<String>) {
    // eprintln!("{lnes:?}");
    // Split city, state, zip if necessary
    // "Syracuse, NY  13202"
    // "2303 Rayburn House Office Building, Washington, DC 20515"
    for idx in (0..lnes.len()).rev() {
        // Skip maps coordinates
        // "32.95129530802494", "-96.73322662705269"
        if lnes[idx].len() > 5 && !lnes[idx].contains('.') && ends_with_5digits(&lnes[idx]) {
            let mut lne = lnes[idx].clone();
            lnes.remove(idx);

            let zip = lne.split_off(lne.len() - 5);
            lnes.insert(idx, zip);

            // Text without zip code.
            for s in lne.split_terminator(',').rev() {
                lnes.insert(idx, s.trim().to_string());
            }
        }
    }
}

pub fn edit_dc(lnes: &mut Vec<String>) {
    // Transform "D.C." -> "DC"
    for idx in 0..lnes.len() {
        if lnes[idx] == "DC" {
            break;
        }
        if lnes[idx] == "D.C." {
            lnes[idx] = "DC".to_string();
            break;
        }
    }
}

pub fn edit_split_bar(lnes: &mut Vec<String>) {
    // "WELLS FARGO PLAZA | 221 N. KANSAS STREET | SUITE 1500", "EL PASO, TX 79901 |"
    for idx in (0..lnes.len()).rev() {
        if lnes[idx].contains('|') {
            let lne = lnes[idx].clone();
            lnes.remove(idx);
            for new_lne in lne.split_terminator('|').rev() {
                if !new_lne.is_empty() {
                    lnes.insert(idx, new_lne.trim().to_string());
                }
            }
        }
    }
}

// pub fn edit_address1(lnes: &mut Vec<String>) {
//     // Transform
//     // "U.S. Federal Building, 220 E Rosser Avenue", "Room 228"
//     // "220 E Rosser Avenue", "Room 228 U.S. Federal Building"

//     // TODO:
//     // for idx in 0..lnes.len() {
//     //     if lnes[idx] == "DC" {
//     //         break;
//     //     }
//     //     if lnes[idx] == "D.C." {
//     //         lnes[idx] = "DC".to_string();
//     //         break;
//     //     }
//     // }
// }

pub fn edit_disjoint_zip(lnes: &mut Vec<String>) {
    // Combine disjointed zip code.
    // "Vidalia, GA 304", "74"
    for idx in (1..lnes.len()).rev() {
        if lnes[idx].len() < 5 && is_all_digits(&lnes[idx]) {
            // let wrds: Vec<&str> = footer[idx].split_whitespace().collect();
            // footer[idx - 1] = format!("{} {}", wrds[1], footer[idx - 1]);
            let lne = lnes.remove(idx);
            lnes[idx - 1] += &lne;
            break;
        }
    }
}

pub fn extract_house_addresses(rep: &House, lnes: &[String]) -> Option<Vec<Address>> {
    // Fetching "https://brandonwilliams.house.gov/"...
    // ["1022 Longworth HOB", "Washington", "DC", "20515", "Syracuse District Office", "The Galleries of Syracuse", "440 South Warren Street", "Suite #706", "Syracuse", "NY", "13202", "Utica District Office", "421 Broad Street", "Suite #7", "Utica", "NY", "13501"]
    // Fetching "https://wilson.house.gov/"...
    // ["2080 Rayburn HOB", "Washington", "DC", "20515", "Miami Gardens Office", "18425 NW 2nd Avenue", "Miami Gardens", "FL", "33169", "West Park Office", "West Park City Hall", "1965 South State Road 7", "West Park", "FL", "33023", "Miami Beach Satellite Office", "1700 Convention Center Drive", "First Floor Suite", "Miami Beach", "FL", "33139"]
    // Fetching "https://nikemawilliams.house.gov/"...
    // ["1406 Longworth HOB", "Washington", "DC", "20515", "Atlanta", "100 Peachtree Street Northwest", "Suite 1920", "Atlanta", "GA", "30303"]
    // Fetching "https://wild.house.gov/"...
    // ["1027 Longworth HOB", "Washington", "DC", "20515", "Allentown Office", "504 Hamilton St.", "Suite 3804", "Allentown", "PA", "18101", "Easton Office", "1 South 3rd Street", "Suite 902", "Easton", "PA", "18042", "Lehighton Office", "1001 Mahoning St.", "Lehighton", "PA", "18235"]
    // Fetching "https://yakym.house.gov/"...
    // ["349 Cannon HOB", "Washington", "DC", "20515", "Mishawaka", "2410 Grape Road", "Suite 2A", "Mishawaka", "IN", "46545", "Rochester", "709 Main Street", "Rochester", "IN", "46975"]

    eprintln!("--- extract_house_addresses: {lnes:?}");

    // Start from the bottom.
    // Search for a five digit zip code.
    let mut adrs: Vec<Address> = Vec::new();
    const LEN_ZIP: usize = 5;
    for (idx, lne) in lnes.iter().enumerate().rev() {
        if lne.len() == LEN_ZIP && ends_with_5digits(lne) {
            // eprintln!("-- extract_house_addresses: idx:{idx}");
            // Start of an address.
            let mut adr = Address::default();
            adr.zip.clone_from(lne);
            adr.state.clone_from(&lnes[idx - 1]);
            adr.city.clone_from(&lnes[idx - 2]);

            // Look for address1.
            // Next line could be address1 or address2.
            // Two lines away starts with a digit?
            let mut has_address2 = false;
            if idx >= 4 {
                if let Some(idx) = lnes[idx - 4].find(|c: char| c.is_ascii_digit()) {
                    has_address2 = idx == 0;
                }
            }
            if has_address2 {
                adr.address2 = Some(lnes[idx - 3].clone());
                adr.address1.clone_from(&lnes[idx - 4]);
            } else {
                // Check for cases:
                //  "U.S. Federal Building, 220 E Rosser Avenue", "Room 228", "Bismarck,", "ND", "58501"
                if adr.zip == "58501" {
                    let vals: Vec<&str> = lnes[idx - 4].split_terminator(',').collect();
                    adr.address1 = vals[1].trim().to_string();
                    adr.address2 = Some(format!("{} {}", vals[0].trim(), lnes[idx - 3]));
                } else {
                    // Standard case.
                    adr.address1.clone_from(&lnes[idx - 3]);
                }
            }
            // adr.address1.clone_from(&lnes[idx - 3]);
            // Disabled suite spliting
            // if lnes[idx - 3].to_uppercase().contains("SUITE") {
            //     adr.suite.clone_from(&lnes[idx - 3]);
            //     // adr.address1.clone_from(&lnes[idx - 4]);
            // } else {
            //     adr.address1.clone_from(&lnes[idx - 3]);
            // }
            adr.last_name.clone_from(&rep.last_name);
            adr.first_name.clone_from(&rep.first_name);
            adrs.push(adr);
        }
    }

    // Deduplicate extracted addresses.
    adrs.sort_unstable();
    adrs.dedup_by(|a, b| a == b);

    eprintln!("{} addresses parsed.", adrs.len());

    if adrs.is_empty() {
        return None;
    }

    Some(adrs)
}

pub fn validate_house_addresses(rep: &House, adrs: &[Address]) -> Result<()> {
    for (idx, adr) in adrs.iter().enumerate() {
        if adr.first_name.is_empty() {
            return Err(anyhow!(
                "house address: first_name empty (idx:{} rep:{:?})",
                idx,
                rep
            ));
        }
        if adr.last_name.is_empty() {
            return Err(anyhow!(
                "house address: last_name empty (idx:{} rep:{:?})",
                idx,
                rep
            ));
        }
        if adr.address1.is_empty() {
            return Err(anyhow!(
                "house address: address1 empty (idx:{} rep:{:?})",
                idx,
                rep
            ));
        }
        // Suite may be empty.
        if adr.city.is_empty() {
            return Err(anyhow!(
                "house address: city empty (idx:{} rep:{:?})",
                idx,
                rep
            ));
        }
        if adr.state.is_empty() {
            return Err(anyhow!(
                "house address: state empty (idx:{} rep:{:?})",
                idx,
                rep
            ));
        }
        if adr.zip.is_empty() {
            return Err(anyhow!(
                "house address: zip empty (idx:{} rep:{:?})",
                idx,
                rep
            ));
        }
    }
    Ok(())
}

pub async fn standardize_addresses(
    adrs: &mut Vec<Address>,
    usps_cli: &mut UspsClient,
) -> Result<()> {
    for idx in (0..adrs.len()).rev() {
        match usps_cli.standardize_address(&mut adrs[idx]).await {
            Ok(_) => {
                // Edit cases:
                // 2743 PERIMETR PKWY BLDG 200 STE 105,STE 105,AUGUSTA,GA
                if let Some(idx) = adrs[idx].address1.find("BLDG") {
                    adrs[idx].address2 = Some(adrs[idx].address1[idx..].to_string());
                    adrs[idx].address1.truncate(idx - 1); // truncate extra space
                }
                // 685 CARNEGIE DR STE 100,,SAN BERNARDINO,CA,92408-3581
                else if let Some(idx) = adrs[idx].address1.find("STE") {
                    adrs[idx].address2 = Some(adrs[idx].address1[idx..].to_string());
                    adrs[idx].address1.truncate(idx - 1); // truncate extra space
                }
            }
            Err(err) => {
                eprintln!("standardize_addresses: err: {}", err);
                // Mitigate failed address standardization.
                // Some non-addresses may be removed here.
                // INVALID ADDRESSES
                // https://amodei.house.gov/
                // Address { first_name: "Mark", last_name: "Amodei", address1: "89511", address2: Some("Elko Contact"), city: "Elko", state: "NV", zip: "89801" }
                if adrs[idx].zip == "89801" {
                    eprintln!("removed invalid address: {:?}", adrs[idx]);
                    adrs.remove(idx);
                } else {
                    return Err(err);
                }
            }
        }
    }

    Ok(())
}

pub fn write_addresses_to_csv<P: AsRef<Path>>(path: P, adrs: &[Address]) -> Result<()> {
    eprintln!("Writing CSV file...");
    let file = File::create(path)?;
    let mut wtr = Writer::from_writer(file);

    // Write the header
    wtr.write_record([
        "first_name",
        "last_name",
        "address1",
        "address2",
        "city",
        "state",
        "zip",
    ])?;

    // Write each address
    for address in adrs {
        wtr.write_record([
            &address.first_name,
            &address.last_name,
            &address.address1,
            address.address2.as_deref().unwrap_or(""),
            &address.city,
            &address.state,
            &address.zip,
        ])?;
    }

    wtr.flush()?;
    Ok(())
}

// A US House of Representative.
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct House {
    pub first_name: String,
    pub last_name: String,
    pub url: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_trim_list_prefix() {
        let mut lines = vec![
            "2312 RAYBURN HOUSE OFFICE BUILDING".to_string(),
            "2430 RAYBURN HOUSE OFFICE BLDG.".to_string(),
            "SOME OTHER LINE".to_string(),
        ];
        edit_hob(&mut lines);
        assert_eq!(
            lines,
            vec![
                "2312 RAYBURN HOB".to_string(),
                "2430 RAYBURN HOB".to_string(),
                "SOME OTHER LINE".to_string(),
            ]
        );
    }

    #[test]
    fn test_concat_two() {
        let mut lines = vec![
            "1107 LONGWORTH HOUSE".to_string(),
            "OFFICE BUILDING".to_string(),
            "SOME OTHER LINE".to_string(),
        ];
        edit_hob(&mut lines);
        assert_eq!(
            lines,
            vec![
                "1107 LONGWORTH HOB".to_string(),
                "SOME OTHER LINE".to_string(),
            ]
        );
    }

    #[test]
    fn test_hob_abbreviation() {
        let mut lines = vec![
            "1119 LONGWORTH H.O.B.".to_string(),
            "ANOTHER LINE".to_string(),
        ];
        edit_hob(&mut lines);
        assert_eq!(
            lines,
            vec!["1119 LONGWORTH HOB".to_string(), "ANOTHER LINE".to_string(),]
        );
    }

    #[test]
    fn test_insert_room_number() {
        let mut lines = vec!["LONGWORTH HOB".to_string(), "ROOM 1027".to_string()];
        edit_hob(&mut lines);
        assert_eq!(lines, vec!["1027 LONGWORTH HOB".to_string(),]);
    }

    #[test]
    fn test_no_modification_needed() {
        let mut lines = vec![
            "SOME RANDOM ADDRESS".to_string(),
            "ANOTHER LINE".to_string(),
        ];
        edit_hob(&mut lines);
        assert_eq!(
            lines,
            vec![
                "SOME RANDOM ADDRESS".to_string(),
                "ANOTHER LINE".to_string(),
            ]
        );
    }

    #[test]
    fn test_single_split() {
        let mut lines = vec![
            "WELLS FARGO PLAZA | 221 N. KANSAS STREET | SUITE 1500".to_string(),
            "EL PASO, TX 79901 |".to_string(),
        ];
        edit_split_bar(&mut lines);
        assert_eq!(
            lines,
            vec![
                "WELLS FARGO PLAZA".to_string(),
                "221 N. KANSAS STREET".to_string(),
                "SUITE 1500".to_string(),
                "EL PASO, TX 79901".to_string(),
            ]
        );
    }

    #[test]
    fn test_no_split() {
        let mut lines = vec!["123 MAIN STREET".to_string(), "SUITE 500".to_string()];
        edit_split_bar(&mut lines);
        assert_eq!(
            lines,
            vec!["123 MAIN STREET".to_string(), "SUITE 500".to_string(),]
        );
    }

    #[test]
    fn test_multiple_splits() {
        let mut lines = vec![
            "PART 1 | PART 2 | PART 3".to_string(),
            "PART A | PART B | PART C".to_string(),
        ];
        edit_split_bar(&mut lines);
        assert_eq!(
            lines,
            vec![
                "PART 1".to_string(),
                "PART 2".to_string(),
                "PART 3".to_string(),
                "PART A".to_string(),
                "PART B".to_string(),
                "PART C".to_string(),
            ]
        );
    }

    #[test]
    fn test_edge_case_trailing_bar() {
        let mut lines = vec!["TRAILING BAR |".to_string(), "| LEADING BAR".to_string()];
        edit_split_bar(&mut lines);
        assert_eq!(
            lines,
            vec!["TRAILING BAR".to_string(), "LEADING BAR".to_string(),]
        );
    }

    #[test]
    fn test_empty_string() {
        let mut lines = vec!["".to_string()];
        edit_split_bar(&mut lines);
        assert_eq!(lines, vec!["".to_string(),]);
    }

    #[test]
    fn test_mixed_content() {
        let mut lines = vec![
            "MIXED CONTENT | 123 | ABC".to_string(),
            "NORMAL LINE".to_string(),
            "ANOTHER | LINE".to_string(),
        ];
        edit_split_bar(&mut lines);
        assert_eq!(
            lines,
            vec![
                "MIXED CONTENT".to_string(),
                "123".to_string(),
                "ABC".to_string(),
                "NORMAL LINE".to_string(),
                "ANOTHER".to_string(),
                "LINE".to_string(),
            ]
        );
    }
}
