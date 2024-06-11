use std::char;

use crate::models::*;
use crate::usps::*;
use anyhow::{anyhow, Result};
use regex::Regex;

pub struct Prsr {
    /// A regex matching a floating point number:
    /// "46.86551919465073", "-96.83144324414937".
    pub re_flt: Regex,
    /// A regex matching initials in a name.
    pub re_name_initials: Regex,
    /// A regex matching abbreviations of US states and US territories according to the USPS.
    pub re_state: Regex,
    /// A regex matching US phone numbers.
    pub re_phone: Regex,
    /// A regex matching an address1.
    pub re_address1: Regex,
    /// A regex matching an address1 suffix such as `Street`.
    pub re_address1_suffix: Regex,
    /// A regex matching a PO Box.
    pub re_po_box: Regex,
    /// A regex matching clock time.
    pub re_time: Regex,
}

impl Prsr {
    pub fn new() -> Self {
        Prsr {
            re_flt: Regex::new(r"^-?\d+\.\d+$").unwrap(),
            re_name_initials: Regex::new(r"\b[A-Z]\.\s+").unwrap(),
            re_state:Regex::new(r"(?xi)  # Case-insensitive and extended modes
            \b(                            # Word boundary and start of group
            AL|Alabama|AK|Alaska|AS|American\s+Samoa|AZ|Arizona|AR|Arkansas|CA|California|
            CO|Colorado|CT|Connecticut|DE|Delaware|DC|District\s+of\s+Columbia|FM|Federated\s+States\s+of\s+Micronesia|
            FL|Florida|GA|Georgia|GU|Guam|HI|Hawaii|ID|Idaho|IL|Illinois|IN|Indiana|
            IA|Iowa|KS|Kansas|KY|Kentucky|LA|Louisiana|ME|Maine|MH|Marshall\s+Islands|
            MD|Maryland|MA|Massachusetts|MI|Michigan|MN|Minnesota|MS|Mississippi|
            MO|Missouri|MT|Montana|NE|Nebraska|NV|Nevada|NH|New\s+Hampshire|NJ|New\s+Jersey|
            NM|New\s+Mexico|NY|New\s+York|NC|North\s+Carolina|ND|North\s+Dakota|MP|Northern\s+Mariana\s+Islands|
            OH|Ohio|OK|Oklahoma|OR|Oregon|PW|Palau|PA|Pennsylvania|PR|Puerto\s+Rico|
            RI|Rhode\s+Island|SC|South\s+Carolina|SD|South\s+Dakota|TN|Tennessee|TX|Texas|
            UT|Utah|VT|Vermont|VI|Virgin\s+Islands|VA|Virginia|WA|Washington|WV|West\s+Virginia|
            WI|Wisconsin|WY|Wyoming|AA|Armed\s+Forces\s+Americas|AE|Armed\s+Forces\s+Europe|AP|Armed\s+Forces\s+Pacific
            )\b                            # End of group and word boundary
        ").unwrap(),
            re_phone: Regex::new(r"(?x)
                ^                        # Start of string
                (?:\+1[-.\s]?)?          # Optional country code
                (?:\(?\d{3}\)?[-.\s])    # Area code with optional parentheses and required separator
                \d{3}[-.\s]?             # First three digits with optional separator
                \d{4}                    # Last four digits
                $                        # End of string
            ").unwrap(),
            re_address1: Regex::new(r"(?xi)
                ^                # Start of string
                (
                    \d+              # One or more digits at the beginning
                    [A-Za-z]?        # Zero or one letter immediately after the initial digits
                    [-\s]?           # Zero or one minus sign or space
                    \d*              # Zero or more digits
                    [A-Za-z]*        # Zero or more letters immediately after the trailing digits
                    /?               # Zero or one slash
                    \d*              # Zero or more digits
                    |                # OR
                    one|two|three|four|five|six|seven|eight|nine|ten|
                    eleven|twelve|thirteen|fourteen|fifteen|sixteen|
                    seventeen|eighteen|nineteen|twenty
                )
                \s+              # One or more spaces after the digits
                .*               # Any characters (including none) in between
                [A-Za-z]         # At least one letter somewhere in the string
                .*               # Any characters (including none) after the letter
                $                # End of string
            ").unwrap(),
            re_address1_suffix: Regex::new(r"(?i)\b(?:ROAD|RD|STREET|ST|AVENUE|AVE|DRIVE|DR|CIRCLE|CIR|BOULEVARD|BLVD|PLACE|PL|COURT|CT|LANE|LN|PARKWAY|PKWY|TERRACE|TER|WAY|WAY|ALLEY|ALY|CRESCENT|CRES|HIGHWAY|HWY|SQUARE|SQ)\b").unwrap(),
            re_po_box: Regex::new(r"(?ix)
                ^                # Start of string
                P \s* \.? \s* O \s* \.? \s* BOX  # Match 'P.O. BOX', 'PO BOX', 'P.O.BOX', 'POBOX' with optional spaces and periods
                \s*              # Zero or more space after 'PO BOX'
                \d+              # One or more digits
                $                # End of string
            ").unwrap(),
            re_time: Regex::new(r"(?i)\b\d{1,2}\s*(?:AM|PM|A\.M\.|P\.M\.)").unwrap(),
        }
    }

    pub fn filter(&self, s: &str) -> bool {
        !s.is_empty()
            && !s.contains("IFRAME")
            && !s.contains("FUNCTION")
            && !s.contains("FORM")
            && !s.contains("!IMPORTANT;")
            && !s.contains("<DIV")
            && !s.contains("<SPAN")
            && !s.contains("HTTPS")
            && !self.re_phone.is_match(s)
            && !self.re_flt.is_match(s)
            && !s.contains("PHONE:")
            && !s.contains("FAX:")
            && !s.contains("OFFICE OF")
            && !s.starts_with("P: ")
            && !s.starts_with("F: ")
            && !contains_time(s)
    }

    pub fn edit_lnes(&self, lnes: &mut Vec<String>) {
        // Edit lines to make it easier to parse.

        edit_split_bar(lnes);
        // eprintln!("(1) {lnes:?}");
        self.edit_concat_zip(lnes);
        // eprintln!("(2) {lnes:?}");
        edit_zip_disjoint(lnes);
        // eprintln!("(3) {lnes:?}");
        self.edit_split_city_state_zip(lnes);
        // eprintln!("(4) {lnes:?}");
        edit_drain_after_last_zip(lnes);
        //eprintln!("(5) {lnes:?}");
        edit_single_comma(lnes);
    }

    pub fn remove_initials(&self, full_name: &str) -> String {
        // Define the regular expression to match initials
        let re = Regex::new(r"\b[A-Z]\.\s+").unwrap();
        // Replace the initials with an empty string
        self.re_name_initials.replace_all(full_name, "").to_string()
    }

    pub fn parse_addresses(&self, per: &Person, lnes: &[String]) -> Option<Vec<Address>> {
        // eprintln!("--- parse_addresses: {lnes:?}");

        // Start from the bottom.
        // Search for a five digit zip code.
        let mut adrs: Vec<Address> = Vec::new();
        for (idx, lne) in lnes.iter().enumerate().rev() {
            if is_zip(lne) && !is_invalid_zip(lne) {
                // eprintln!("-- parse_addresses: idx:{idx}");
                // Start of an address.
                let mut adr = Address::default();
                adr.zip.clone_from(lne);
                adr.state.clone_from(&lnes[idx - 1]);
                let idx_city = idx - 2;
                adr.city.clone_from(&lnes[idx_city]);

                // Address1.
                // Starts with digit and contains letter.
                // Next line could be address1 or address2.
                // ["610 MAIN STREET","FIRST FLOOR SMALL","CONFERENCE ROOM","JASPER","IN","47547"]
                // 1710 ALABAMA AVENUE,247 CARL ELLIOTT BUILDING,JASPER,AL,35501
                // PO BOX 729,SUITE # I-10,BELTON,TX,76513
                // "300 EAST 8TH ST, 7TH FLOOR", "AUSTIN", "TX",
                let mut idx_adr1 = idx.saturating_sub(3);
                while idx_adr1 != usize::MAX
                    && !(self.re_address1.is_match(&lnes[idx_adr1])
                        || self.re_po_box.is_match(&lnes[idx_adr1]))
                {
                    idx_adr1 = idx_adr1.wrapping_sub(1);
                }
                if idx_adr1 == usize::MAX {
                    eprintln!("Unable to find address line 1 {}", adr);
                    return None;
                }
                // Check if address2 looks like address1.
                if idx_adr1 != 0
                    && !self.re_po_box.is_match(&lnes[idx_adr1])
                    && self.re_address1.is_match(&lnes[idx_adr1 - 1])
                {
                    idx_adr1 -= 1;
                }
                adr.address1.clone_from(&lnes[idx_adr1]);

                // Address2, if any.
                // If multiple lines, concatenate.
                let mut idx_adr2 = idx_adr1 + 1;
                if idx_adr2 != idx_city {
                    let mut address2 = lnes[idx_adr2].clone();
                    idx_adr2 += 1;
                    while idx_adr2 != idx_city {
                        address2.push(' ');
                        address2.push_str(&lnes[idx_adr2]);
                        idx_adr2 += 1;
                    }
                    adr.address2 = Some(address2);
                }
                adrs.push(adr);
            }
        }

        // Most have one office in state and DC.
        // US Virgin Islands has 1 valid address in DC.
        if adrs.is_empty() {
            return None;
        }

        // Deduplicate extracted addresses.
        adrs.sort_unstable();
        adrs.dedup_by(|a, b| a == b);

        eprintln!("{} addresses parsed.", adrs.len());

        Some(adrs)
    }

    pub fn edit_concat_zip(&self, lnes: &mut Vec<String>) {
        // Concat single zip code for later parsing.
        // "355 S. WASHINGTON ST, SUITE 210, DANVILLE, IN", "46122" ->
        // "355 S. WASHINGTON ST, SUITE 210, DANVILLE, IN 46122"
        // Invalid concat: "PR", "00902-3958"
        for idx in (1..lnes.len()).rev() {
            let lne = lnes[idx].clone();
            if is_zip(&lne) && !self.re_state.is_match(&lnes[idx - 1]) {
                lnes[idx - 1].push(' ');
                lnes[idx - 1].push_str(&lne);
                lnes.remove(idx);
            }
        }
    }

    pub fn lnes_have_zip(&self, lnes: &[String]) -> bool {
        for lne in lnes {
            if is_zip(lne) {
                return true;
            }
        }
        false
    }

    pub fn edit_split_city_state_zip(&self, lnes: &mut Vec<String>) {
        // Split city, state, zip if necessary
        //  "Syracuse, NY  13202"
        //  "2303 Rayburn House Office Building, Washington, DC 20515"
        //  "615 E. WORTHY STREET GONZALES, LA 70737"
        //  "SOMERTON AZ 85350"
        //  "GARNER NC, 27529"
        //  "ST. THOMAS, VI 00802"
        // Invalid split: "P.O. BOX 9023958", "SAN JUAN", "PR", "00902-3958"

        for idx in (0..lnes.len()).rev() {
            let mut lne = lnes[idx].clone();
            if let Some(zip) = ends_with_zip(&lne) {
                // Remove current line.
                lnes.remove(idx);
                lne.truncate(lne.len() - zip.len());
                // Insert zip.
                lnes.insert(idx, zip);

                // Look for state.
                // Cannot rely on comma placement.
                // Look for last match.
                // Possible city and state have same name, "Washington".
                if let Some(mat) = self.re_state.find_iter(&lne).last() {
                    // Insert state.
                    lnes.insert(idx, mat.as_str().into());
                    lne.truncate(mat.start());
                    trim_end_spc_pnc(&mut lne);
                }

                if lne.contains(',') {
                    for mut prt in lne.split_terminator(',').rev() {
                        lnes.insert(idx, prt.trim().into());
                    }

                    // let mut saw_state = false;
                    // for mut prt in lne.split_terminator(',').rev() {
                    //     prt = prt.trim();
                    //     if saw_state {
                    //         // Check if street and city not delimited.
                    //         // 615 E WORTHY STREET GONZALES
                    //         // 430 NORTH FRANKLIN ST FORT BRAGG, CA 95437
                    //         if let Some(mat) = self.re_address1_suffix.find(prt) {
                    //             if mat.end() != prt.len() {
                    //                 // Split street from city.
                    //                 let (adr1, city) = prt.split_at(mat.end());
                    //                 lnes.insert(idx, city.trim().into());
                    //                 lnes.insert(idx, adr1.trim().into());
                    //             } else {
                    //                 // Regular address line.
                    //                 lnes.insert(idx, prt.into());
                    //             }
                    //         } else {
                    //             // Regular address part.
                    //             lnes.insert(idx, prt.into());
                    //         }
                    //     } else if self.re_state.is_match(prt) {
                    //         // Insert state.
                    //         lnes.insert(idx, prt.into());
                    //         saw_state = true;
                    //     }
                    // }
                } else {
                    // Check if street and city not delimited.
                    // 615 E WORTHY STREET GONZALES
                    // 430 NORTH FRANKLIN ST FORT BRAGG, CA 95437
                    // "GLEN ALLEN, VA 23060"
                    // "SAN LUIS OBISPO, CA 93401"
                    lnes.insert(idx, lne);

                    //--
                    // let spc_cnt = lne.chars().filter(|c| c.is_whitespace()).count();
                    // if spc_cnt < 2 || lne == "SAN LUIS OBISPO" {
                    //     lnes.insert(idx, lne);
                    // } else {
                    //     for mut prt in lne.split_whitespace().rev() {
                    //         lnes.insert(idx, prt.into());
                    //     }
                    // }

                    //--
                    // match lne.as_str() {
                    //     "ST THOMAS" | "LAS VEGAS" | "SARATOGA SPRINGS" | "LAKE JACKSON"
                    //     | "LEAGUE CITY" => {
                    //         lnes.insert(idx, lne);
                    //     }
                    //     _ => {
                    //         // "SOMERTON AZ 85350"
                    //         for mut prt in lne.split_whitespace().rev() {
                    //             lnes.insert(idx, prt.into());
                    //         }
                    //     }
                    // }
                }
            }
        }
    }
}

pub fn filter_invalid_addresses(per: &Person, adrs: &mut Vec<Address>) -> Result<()> {
    for idx in (0..adrs.len()).rev() {
        if adrs[idx].address1 == "146 N STATE AVENUE" && adrs[idx].city == "SOMERTON" {
            adrs.remove(idx);
        } else if adrs[idx].address1 == "27 INDEPENDENCE AVE SE" && adrs[idx].zip == "20003" {
            // TODO: FIX? OR REMOVE "27 INDEPENDENCE AVE SE"?
            // ADDRESS CAN APPLY TO MANY
            adrs[idx].address1 = "143 CANNON HOB".into();
            adrs[idx].city = "WASHINGTON".into();
            adrs[idx].state = "DC".into();
            adrs[idx].zip = "20515".into();
        }
    }

    Ok(())
}

pub fn validate_addresses(per: &Person, adrs: &[Address]) -> Result<()> {
    for (idx, adr) in adrs.iter().enumerate() {
        if adr.address1.is_empty() {
            return Err(anyhow!("address: address1 empty {:?}", adr));
        }
        // address2 may be None.
        if adr.city.is_empty() {
            return Err(anyhow!("address: city empty {:?}", adr));
        }
        if adr.state.is_empty() {
            return Err(anyhow!("address: state empty {:?}", adr));
        }
        if adr.zip.is_empty() {
            return Err(anyhow!("address: zip empty {:?}", adr));
        }
    }

    Ok(())
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

pub fn edit_drain_after_last_zip(lnes: &mut Vec<String>) {
    // Trim the list after the last zip code.
    // Search for the last zip code.
    for idx in (0..lnes.len()).rev() {
        if is_zip(&lnes[idx]) {
            lnes.drain(idx + 1..);
            break;
        }
    }
}

pub fn edit_hob(lnes: &mut Vec<String>) {
    // Trim list prefix prior to "House Office Building"
    // Reverse indexes to allow for room line removal.
    for idx in (0..lnes.len()).rev() {
        if lnes[idx].starts_with("45 INDEPENDENCE AVE")
            || lnes[idx].starts_with("15 INDEPENDENCE AVE")
            || lnes[idx].starts_with("27 INDEPENDENCE AVE")
        {
            lnes.remove(idx);
        }

        if !(lnes[idx].contains("CANNON")
            || lnes[idx].contains("LONGWORTH")
            || lnes[idx].contains("RAYBURN"))
        {
            continue;
        }

        // "RAYBURN HOUSE OFFICE BUILDING, 2419"
        if let Some(idx_fnd) = lnes[idx].find(',') {
            let lne = lnes[idx].clone();
            lnes[idx] = lne[idx_fnd + 1..].trim().to_string();
            lnes[idx].push(' ');
            lnes[idx].push_str(&lne[..idx_fnd]);
        }

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
        if let Some(idx_fnd) = lnes[idx].find("HOUSE OFFICE") {
            lnes[idx].truncate(idx_fnd);
            lnes[idx].push_str("HOB");
            // No break. Can have duplicate addresses.
        }

        // 2205 RAYBURN OFFICE BUILDING
        if let Some(idx_fnd) = lnes[idx].find("OFFICE BUILDING") {
            lnes[idx].truncate(idx_fnd);
            lnes[idx].push_str("HOB");
            // No break. Can have duplicate addresses.
        }

        // 2205 RAYBURN BUILDING
        if let Some(idx_fnd) = lnes[idx].find("BUILDING") {
            lnes[idx].truncate(idx_fnd);
            lnes[idx].push_str("HOB");
            // No break. Can have duplicate addresses.
        }

        // // TODO: DELETE AND CHECK
        // // "1119 LONGWORTH H.O.B."
        // // "H.O.B." -> "HOB"
        // if let Some(idx_fnd) = lnes[idx].find("H.O.B.") {
        //     lnes[idx].truncate(idx_fnd);
        //     lnes[idx].push_str("HOB");
        //     // No break. Can have duplicate addresses.
        // }

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

pub fn edit_dot(lnes: &mut [String]) {
    // Remove dots.
    // "D.C." -> "DC"
    // "2004 N. CLEVELAND ST." -> "2004 N CLEVELAND ST"
    for lne in lnes.iter_mut() {
        if lne.contains('.') {
            *lne = lne.replace('.', "");
        }
    }
}

pub fn edit_single_comma(lnes: &mut Vec<String>) {
    // Remove single comma.
    // "," -> DELETE
    for idx in (0..lnes.len()).rev() {
        if lnes[idx] == "," {
            lnes.remove(idx);
        }
    }
}

pub fn edit_split_comma(lnes: &mut Vec<String>) {
    // Remove dots.
    // "U.S. FEDERAL BUILDING, 220 E ROSSER AVENUE" ->
    // "U.S. FEDERAL BUILDING" "220 E ROSSER AVENUE"
    for idx in (0..lnes.len()).rev() {
        if lnes[idx].contains(',') {
            let lne = lnes[idx].clone();
            for s in lne.split(|c: char| c == ',').rev() {
                lnes.insert(idx + 1, s.trim().to_string());
            }
            lnes.remove(idx);
        }
    }
}

pub fn edit_person_lnes(per: &Person, lnes: &mut Vec<String>) {
    match (per.name_fst.as_str(), per.name_lst.as_str()) {
        ("Matthew", "Rosendale") => {
            for idx in (0..lnes.len()).rev() {
                if lnes[idx] == "3300 2ND AVENUE N SUITES 7-8" {
                    lnes[idx] = "3300 2ND AVENUE N SUITE 7".into();
                }
            }
        }
        ("Terri", "Sewell") => {
            for idx in (0..lnes.len()).rev() {
                if lnes[idx] == "101 SOUTH LAWRENCE ST COURTHOUSE ANNEX 3" {
                    lnes[idx] = "101 SOUTH LAWRENCE ST".into();
                }
            }
        }
        ("Joe", "Wilson") => {
            for idx in (0..lnes.len()).rev() {
                if lnes[idx] == "1700 SUNSET BLVD (US 378), SUITE 1" {
                    lnes[idx] = "1700 SUNSET BLVD STE 1".into();
                }
            }
        }
        ("Robert", "Wittman") => {
            for idx in (0..lnes.len()).rev() {
                if lnes[idx] == "508 CHURCH LANE" || lnes[idx] == "307 MAIN STREET" {
                    lnes.remove(idx);
                }
            }
        }
        ("Andy", "Biggs") => {
            for idx in (0..lnes.len()).rev() {
                if lnes[idx] == "SUPERSTITION PLAZA" {
                    lnes.remove(idx);
                }
            }
        }
        ("John", "Carter") => {
            for idx in (0..lnes.len()).rev() {
                if lnes[idx] == "SUITE # I-10" {
                    lnes.remove(idx);
                }
            }
        }
        ("Michael", "Cloud") => {
            for idx in (0..lnes.len()).rev() {
                if lnes[idx] == "TOWER II" {
                    lnes.remove(idx);
                }
            }
        }
        ("Tony", "Gonzales") => {
            for idx in (0..lnes.len()).rev() {
                if lnes[idx].contains("(BY APPT ONLY)") {
                    lnes[idx] = lnes[idx].replace(" (BY APPT ONLY)", "");
                }
            }
        }
        ("Garret", "Graves") => {
            for idx in (0..lnes.len()).rev() {
                if lnes[idx].contains("615 E WORTHY STREET GONZALES") {
                    lnes[idx] = "GONZALES".into();
                    lnes.insert(idx, "615 E WORTHY ST".into());
                }
            }
        }
        ("Jared", "Huffman") => {
            for idx in (0..lnes.len()).rev() {
                if lnes[idx] == "430 NORTH FRANKLIN ST FORT BRAGG, CA 95437" {
                    lnes[idx] = "FORT BRAGG, CA 95437".into();
                    lnes.insert(idx, "430 NORTH FRANKLIN ST".into());
                } else if lnes[idx].contains("FORT BRAGG 95437") {
                    lnes[idx] = "FORT BRAGG, CA 95437".into();
                }
            }
        }
        ("Bill", "Huizenga") => {
            for idx in (0..lnes.len()).rev() {
                if lnes[idx].contains("108 PORTAGE, MI 49002") {
                    lnes[idx] = lnes[idx].replace("108 PORTAGE, MI 49002", "108\nPORTAGE, MI 49002")
                }
            }
        }
        ("Mike", "Johnson") => {
            for idx in (0..lnes.len()).rev() {
                if lnes[idx] == "444 CASPARI DRIVE" || lnes[idx] == "SOUTH HALL ROOM 224" {
                    lnes.remove(idx);
                } else if lnes[idx] == "PO BOX 4989 (MAILING)" {
                    lnes[idx] = "PO BOX 4989".into();
                }
            }
        }
        ("Michael", "Lawler") => {
            for idx in (0..lnes.len()).rev() {
                if lnes[idx] == "PO BOX 1645" {
                    lnes.remove(idx);
                }
            }
        }
        ("Anna Paulina", "Luna") => {
            for idx in (0..lnes.len()).rev() {
                if lnes[idx].contains("OFFICE SUITE:") {
                    lnes[idx] = lnes[idx].replace("OFFICE SUITE:", "STE")
                }
            }
        }
        ("Daniel", "Meuser") => {
            for idx in (0..lnes.len()).rev() {
                if lnes[idx] == "SUITE 110, LOSCH PLAZA" {
                    lnes[idx] = "SUITE 110".into();
                }
            }
        }
        ("Max", "Miller") => {
            for idx in (0..lnes.len()).rev() {
                if lnes[idx] == "WASHINGTON" && idx != 0 {
                    lnes.insert(idx - 1, "143 CANNON HOB".into());
                    break;
                }
            }
        }
        ("Frank", "Pallone") => {
            for idx in (0..lnes.len()).rev() {
                if lnes[idx] == "67/69 CHURCH ST" {
                    lnes[idx] = "67 CHURCH ST".into();
                }
            }
        }
        ("Stacey", "Plaskett") => {
            for idx in (0..lnes.len()).rev() {
                if lnes[idx] == "FREDERIKSTED, VI 00840" {
                    lnes[idx] = "ST CROIX, VI 00840".into();
                }
            }
        }
        ("", "") => {}
        _ => {}
    }
}

// TODO: MOVE TO edit_person_lnes.
//  FIND PERSON
pub fn edit_zip_disjoint(lnes: &mut Vec<String>) {
    // Combine disjointed zip code.
    // "Vidalia, GA 304", "74"
    for idx in (1..lnes.len()).rev() {
        if lnes[idx].len() < 5 && lnes[idx].chars().all(|c| c.is_ascii_digit()) {
            let lne = lnes.remove(idx);
            lnes[idx - 1] += &lne;
            break;
        }
    }
}

pub fn edit_mailing(lnes: &mut [String]) {
    // Remove "MAILING ADDRESS:".
    // "MAILING ADDRESS: PO BOX4105" -> "PO BOX4105"
    const MAILING: &str = "MAILING ADDRESS:";
    for lne in lnes.iter_mut() {
        if lne.starts_with(MAILING) {
            *lne = lne[MAILING.len()..].trim().to_string();
        }
    }
}

pub fn edit_office_suite(lnes: &mut [String]) {
    // Replace "OFFICE SUITE:".
    // "9200 113TH ST. N. OFFICE SUITE: 305" ->
    // "9200 113TH ST. N. STE 305"
    const OFFICE_SUITE: &str = "OFFICE SUITE:";
    const STE: &str = "STE";
    for idx in (0..lnes.len()).rev() {
        if lnes[idx].contains(OFFICE_SUITE) {
            lnes[idx] = lnes[idx].replace(OFFICE_SUITE, STE);
        }
    }
}

pub fn edit_starting_hash(lnes: &mut [String]) {
    // Remove (#).
    // "#3 TENNESSEE AVENUE" -> "3 TENNESSEE AVENUE"
    for lne in lnes.iter_mut() {
        if lne.starts_with('#') && lne.len() > 1 {
            *lne = lne[1..].to_string();
        }
    }
}

pub fn edit_char_half(lnes: &mut [String]) {
    // Replace (½).
    // "1411 ½ AVERSBORO RD" -> "1411 1/2 AVERSBORO RD"
    for lne in lnes.iter_mut() {
        if lne.contains('½') {
            *lne = lne.replace('½', "1/2")
        }
    }
}

pub fn edit_empty(lnes: &mut Vec<String>) {
    for idx in (0..lnes.len()).rev() {
        if lnes[idx].is_empty() {
            lnes.remove(idx);
        }
    }
}

pub fn edit_newline(lnes: &mut Vec<String>) {
    // Remove unicode.
    // "154 CANNON HOUSE OFFICE BUILDING\n\nWASHINGTON, \nDC\n20515"
    for idx in (0..lnes.len()).rev() {
        if lnes[idx].contains('\n') {
            let segs: Vec<String> = lnes[idx]
                .split_terminator('\n')
                .filter(|s| !s.is_empty())
                .map(|s| s.trim().trim_end_matches(',').to_string())
                .collect();
            lnes.remove(idx);
            segs.into_iter().rev().for_each(|s| lnes.insert(idx, s));
        }
    }
}

/// Zip codes associated with addresses the USPS does not recognize.
pub fn is_invalid_zip(zip: &str) -> bool {
    matches!(
        zip,
        "89801"
            | "49854"
            | "78702"
            | "29142"
            | "85139"
            | "78071"
            | "07410"
            | "85353"
            | "12451"
            | "28562"
            | "00802"
            | "96952"
    )
}

pub const LEN_ZIP5: usize = 5;
pub const LEN_ZIP10: usize = 10;
pub const ZIP_DASH: char = '-';

/// Checks whether a string is a USPS zip with 5 digits or 9 digits.
pub fn is_zip(lne: &str) -> bool {
    // 12345, 12345-6789
    match lne.len() {
        LEN_ZIP5 => lne.chars().all(|c| c.is_ascii_digit()),
        LEN_ZIP10 => lne.chars().enumerate().all(|(idx, c)| {
            if idx == LEN_ZIP5 {
                c == ZIP_DASH
            } else {
                c.is_ascii_digit()
            }
        }),
        _ => false,
    }
}

/// Checks whether a string is a USPS zip with 5 characters, `12345`.
pub fn is_zip5(lne: &str) -> bool {
    lne.len() == LEN_ZIP5 && lne.chars().all(|c| c.is_ascii_digit())
}

/// Checks whether a string is a USPS zip with 10 characters, `12345-6789`.
pub fn is_zip10(lne: &str) -> bool {
    if lne.len() != LEN_ZIP10 {
        return false;
    }
    lne.chars().enumerate().all(|(idx, c)| {
        if idx == LEN_ZIP5 {
            c == ZIP_DASH
        } else {
            c.is_ascii_digit()
        }
    })
}

/// Checks whether a string ends with a USPS zip with 5 characters.
///
/// Specified string expected to be longer than 5 characters.
pub fn ends_with_zip5(lne: &str) -> Option<String> {
    // Disallow exact match.
    if lne.len() > LEN_ZIP5 {
        // Check 5 digit zip.
        let zip: String = lne.chars().skip(lne.chars().count() - LEN_ZIP5).collect();
        if is_zip5(&zip) {
            // Edge case checks.
            // Check for too many digits, 123456.
            // Check for invalid zip, 12345-67890.
            if let Some(c) = lne.chars().rev().nth(LEN_ZIP5) {
                if !c.is_ascii_digit() && c != ZIP_DASH {
                    return Some(zip);
                }
            }
        }
    }

    None
}

/// Checks whether a string ends with a USPS zip with 10 characters.
///
/// Specified string expected to be longer than 10 characters.
pub fn ends_with_zip10(lne: &str) -> Option<String> {
    // Disallow exact match.
    if lne.len() > LEN_ZIP10 {
        // Check 10 digit zip.
        let zip: String = lne.chars().skip(lne.chars().count() - LEN_ZIP10).collect();
        if is_zip10(&zip) {
            return Some(zip);
        }
    }

    None
}

/// Checks whether a string ends with a USPS zip with 5 characters or 10 characters.
pub fn ends_with_zip(lne: &str) -> Option<String> {
    match ends_with_zip5(lne) {
        Some(zip) => Some(zip),
        None => ends_with_zip10(lne),
    }
}

/// Checks whether the string contains clock time, 9AM, 5 p.m.
pub fn contains_time(lne: &str) -> bool {
    let mut lft: usize = 0;

    let mut saw_fst_chr = false;
    let mut cnt_dig: u8 = 0;
    for c in lne.chars() {
        if cnt_dig > 0 {
            // Skip all whitespace.
            if c.is_whitespace() {
                continue;
            }
            // Count digits.
            if c.is_ascii_digit() {
                // Check for too many digits.
                // Invalid: 123 AM
                if cnt_dig == 2 {
                    // Reset search for start of pattern.
                    cnt_dig = 0;
                    continue;
                }
                // Count second digit.
                cnt_dig = 2;
            }

            if saw_fst_chr {
                // Skip over dot
                if c == '.' {
                    continue;
                }

                if c == 'M' || c == 'm' {
                    return true;
                } else {
                    // Reset search for start of pattern.
                    cnt_dig = 0;
                }
            } else if c == 'A' || c == 'a' || c == 'P' || c == 'p' {
                saw_fst_chr = true;
            } else if !c.is_ascii_digit() {
                // Reset search for start of pattern.
                cnt_dig = 0;
            }
        } else if c.is_ascii_digit() {
            // Count first digit.
            cnt_dig = 1;
        }
    }

    false
}

/// Trim space and punctuation from the end of a string.
pub fn trim_end_spc_pnc(lne: &mut String) {
    let chars: Vec<char> = lne.chars().collect();

    // Find the index where the non-whitespace and non-punctuation starts
    let trim_idx = chars
        .iter()
        .rposition(|&c| !c.is_whitespace() && !c.is_ascii_punctuation())
        .map_or(0, |pos| pos + 1);

    // Return the trimmed string
    // chars[..trim_idx].iter().collect()
    lne.truncate(trim_idx);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_regex_po_box_valid() {
        let prsr = Prsr::new();

        let valid_addresses = vec![
            "PO BOX 123",
            "P.O. BOX 456",
            "POBOX789",
            "P.O.BOX 1011",
            "PO BOX1234",
            "PO BOX 5678",
            "P.O. BOX 9023958",
            "PO BOX 9023958",
        ];

        for address in valid_addresses {
            assert!(
                prsr.re_po_box.is_match(address),
                "Failed to match: {}",
                address
            );
        }
    }

    #[test]
    fn test_regex_address1_valid() {
        let prsr = Prsr::new();

        let valid_addresses = vec![
            "403-1/2 NE JEFFERSON STREET",
            "118-B CARLISLE ST",
            "ONE BLUE HILL PLAZA",
            "21-00 NJ 208 S",
            "123 Main St",
            "456 Elm St Apt 7",
            "340A 9TH STREET",
            "10 Downing Street",
            "1024 E 7th St",
        ];

        for address in valid_addresses {
            assert!(
                prsr.re_address1.is_match(address),
                "Failed to match: {}",
                address
            );
        }
    }

    #[test]
    fn test_regex_address1_invalid() {
        let prsr = Prsr::new();

        let invalid_addresses = vec![
            "Main St",
            "Elm St Apt 7",
            "Broadway",
            "Downing Street",
            "Avenue",
            " E 7th St",
            "#508 HARLEM STATE OFFICE BUILDING",
        ];

        for address in invalid_addresses {
            assert!(
                !prsr.re_address1.is_match(address),
                "Incorrectly matched: {}",
                address
            );
        }
    }

    #[test]
    fn test_regex_address1_suffix_valid() {
        let prsr = Prsr::new();

        let valid_cases = vec![
            "123 Main Street",
            "456 Elm St",
            "789 Oak Avenue",
            "101 Pine Ave",
            "202 Maple Drive",
            "303 Cedar Dr",
            "404 Birch Circle",
            "505 Spruce Cir",
            "606 Willow Boulevard",
            "707 Aspen Blvd",
            "808 Birch Place",
            "909 Fir Pl",
            "1234 Cedar Court",
            "5678 Maple Ct",
            "91011 Elm Lane",
            "121314 Oak Ln",
            "151617 Pine Parkway",
            "181920 Spruce Pkwy",
            "212223 Birch Terrace",
            "242526 Cedar Ter",
            "272829 Maple Way",
            "303132 Oak Way",
            "333435 Pine Alley",
            "363738 Spruce Aly",
            "394041 Birch Crescent",
            "424344 Cedar Cres",
            "454647 Maple Highway",
            "484950 Oak Hwy",
            "515253 Pine Square",
            "545556 Spruce Sq",
        ];

        for case in valid_cases {
            assert!(
                prsr.re_address1_suffix.is_match(case),
                "Failed to match valid address suffix in: {}",
                case
            );
        }
    }

    #[test]
    fn test_regex_address1_suffix_invalid() {
        let prsr = Prsr::new();

        let invalid_cases = vec![
            "123 Main Roadway",
            "456 Elm Strt",
            "789 Oak Av",
            "101 Pine Aven",
            "202 Maple Drv",
            "303 Cedar Circl",
            "404 Birch Boulev",
            "505 Spruce Plce",
            "606 Willow Courtyard",
            "707 Aspen Lan",
            "808 Birch Terr",
            "909 Fir Parkwayy",
            "1234 Cedar Waystreet",
            "5678 Maple",
            "91011 Elm Streetdrive",
        ];

        for case in invalid_cases {
            assert!(
                !prsr.re_address1_suffix.is_match(case),
                "Incorrectly matched invalid address suffix in: {}",
                case
            );
        }
    }

    #[test]
    fn test_regex_phone_valid() {
        let prsr = Prsr::new();

        let valid_numbers = vec![
            "202-225-4735",
            "202.225.4735",
            "202 225 4735",
            "(202) 225-4735",
            "+1-202-225-4735",
            "+1 202 225 4735",
            "+1.202.225.4735",
            "+1 (202) 225-4735",
        ];

        for number in valid_numbers {
            assert!(
                prsr.re_phone.is_match(number),
                "Failed to match: {}",
                number
            );
        }
    }

    #[test]
    fn test_regex_phone_invalid() {
        let prsr = Prsr::new();

        let invalid_inputs = vec![
            "12345",             // Zip code
            "12345-6789",        // Zip code
            "789Broadway",       // No separators
            "10 Downing Street", // Not a phone number
        ];

        for input in invalid_inputs {
            assert!(
                !prsr.re_phone.is_match(input),
                "Incorrectly matched: {}",
                input
            );
        }
    }

    #[test]
    fn test_is_zip5_valid() {
        let valid_cases = vec![
            "12345", // Five-digit zip code
            "67890", // Another five-digit zip code
        ];

        for case in valid_cases {
            assert!(is_zip5(case), "Failed to match valid zip code: {}", case);
        }
    }

    #[test]
    fn test_is_zip5_invalid() {
        let invalid_cases = vec![
            "1234",         // Less than five digits
            "123456",       // More than five digits without hyphen
            "12-567",       // Less than five digits before hyphen
            "ABCDE",        // Leters
            "12345-678",    // Less than four digits after hyphen
            "12345-67890",  // More than four digits after hyphen
            "12345 6789",   // Space instead of hyphen
            "12a45-6789",   // Alphabetic character in zip code
            "12345-678a",   // Alphabetic character in extended part
            "123456789",    // No hyphen in extended zip code
            "202-225-4735", // Phone number
        ];

        for case in invalid_cases {
            assert!(
                !is_zip(case),
                "Incorrectly matched invalid zip code: {}",
                case
            );
        }
    }

    #[test]
    fn test_is_zip10_valid() {
        let valid_cases = vec![
            "12345-6789", // Nine-digit zip code
            "98765-4321", // Another nine-digit zip code
        ];

        for case in valid_cases {
            assert!(is_zip10(case), "Failed to match valid zip code: {}", case);
        }
    }

    #[test]
    fn test_is_zip10_invalid() {
        let invalid_cases = vec![
            "1234",         // Less than five digits
            "123456",       // More than five digits without hyphen
            "1234-5678",    // Less than five digits before hyphen
            "12345-678",    // Less than four digits after hyphen
            "12345-67890",  // More than four digits after hyphen
            "12345 6789",   // Space instead of hyphen
            "12a45-6789",   // Alphabetic character in zip code
            "12345-678a",   // Alphabetic character in extended part
            "123456789",    // No hyphen in extended zip code
            "202-225-4735", // Phone number
        ];

        for case in invalid_cases {
            assert!(
                !is_zip10(case),
                "Incorrectly matched invalid zip code: {}",
                case
            );
        }
    }

    #[test]
    fn test_is_zip_valid() {
        let valid_cases = vec![
            "12345",      // Five-digit zip code
            "67890",      // Another five-digit zip code
            "12345-6789", // Nine-digit zip code
            "98765-4321", // Another nine-digit zip code
        ];

        for case in valid_cases {
            assert!(is_zip(case), "Failed to match valid zip code: {}", case);
        }
    }

    #[test]
    fn test_is_zip_invalid() {
        let invalid_cases = vec![
            "1234",         // Less than five digits
            "123456",       // More than five digits without hyphen
            "1234-5678",    // Less than five digits before hyphen
            "12345-678",    // Less than four digits after hyphen
            "12345-67890",  // More than four digits after hyphen
            "12345 6789",   // Space instead of hyphen
            "12a45-6789",   // Alphabetic character in zip code
            "12345-678a",   // Alphabetic character in extended part
            "123456789",    // No hyphen in extended zip code
            "202-225-4735", // Phone number
        ];

        for case in invalid_cases {
            assert!(
                !is_zip(case),
                "Incorrectly matched invalid zip code: {}",
                case
            );
        }
    }

    #[test]
    fn test_ends_with_zip5_valid() {
        let cases = vec![
            ("Address with zip 12345", "12345".into()),
            ("End with 54321", "54321".into()),
            ("Starts with zip 98765", "98765".into()),
            ("Zip in the middle 12345", "12345".into()),
        ];

        for (input, expected) in cases {
            assert_eq!(
                ends_with_zip5(input),
                Some(expected),
                "Failed for input: {}",
                input
            );
        }
    }

    #[test]
    fn test_ends_with_zip5_invalid() {
        let cases = vec![
            "123456",                           // Too many digits
            "Address with 1234",                // Less than 5 digits
            "Zip 1234-5678",                    // Invalid zip with too many digits after dash
            "Random text",                      // No zip code
            "45678-1234",                       // Valid 9-digit zip
            "Address with zip code 12345-6789", // Valid 9-digit zip
            "P.O. BOX 9023958",
        ];

        for input in cases {
            assert_eq!(ends_with_zip5(input), None, "Failed for input: {}", input);
        }
    }

    #[test]
    fn test_ends_with_zip10_valid() {
        let cases = vec![
            ("Address with zip 12345-6789", "12345-6789".into()),
            ("Another one 98765-4321", "98765-4321".into()),
            ("Some text 54321-1234", "54321-1234".into()),
            ("Zip code at end 12345-6789", "12345-6789".into()),
        ];

        for (input, expected) in cases {
            assert_eq!(
                ends_with_zip10(input),
                Some(expected),
                "Failed for input: {}",
                input
            );
        }
    }

    #[test]
    fn test_ends_with_zip10_invalid() {
        let cases = vec![
            "1234567890",             // Exactly 10 digits without dash
            "Address with 12345-678", // Less than 4 digits after dash
            "Text with 12345-67890",  // More than 4 digits after dash
            "Random text",            // No zip code
            "Another text 123456",    // Only 6 digits
            "Invalid zip 1234-56789", // Only 4 digits before dash
            "P.O. BOX 9023958",
        ];

        for input in cases {
            assert_eq!(ends_with_zip10(input), None, "Failed for input: {}", input);
        }
    }

    #[test]
    fn test_ends_with_zip_valid() {
        let cases = vec![
            ("Address with zip 12345", "12345".into()),
            ("Another one 98765-4321", "98765-4321".into()),
            ("Some text 54321", "54321".into()),
            ("Zip code at end 12345-6789", "12345-6789".into()),
            ("Ends with zip 54321-1234", "54321-1234".into()),
            ("Starts with zip 98765", "98765".into()),
        ];

        for (input, expected) in cases {
            assert_eq!(
                ends_with_zip(input),
                Some(expected),
                "Failed for input: {}",
                input
            );
        }
    }

    #[test]
    fn test_ends_with_zip_invalid() {
        let cases = vec![
            "123456",                  // Exactly 6 digits without dash
            "1234567890",              // Exactly 10 digits without dash
            "Address with 1234",       // Less than 5 digits
            "Text with 12345-678",     // Less than 4 digits after dash
            "Random text",             // No zip code
            "Another text 1234-56789", // Only 4 digits before dash
            "P.O. BOX 9023958",
        ];

        for input in cases {
            assert_eq!(ends_with_zip(input), None, "Failed for input: {}", input);
        }
    }

    #[test]
    fn test_regex_state_valid() {
        let prsr = Prsr::new();

        let valid_entries = vec![
            "AL",
            "Alabama",
            "AK",
            "Alaska",
            "AS",
            "American Samoa",
            "AZ",
            "Arizona",
            "AR",
            "Arkansas",
            "CA",
            "California",
            "CO",
            "Colorado",
            "CT",
            "Connecticut",
            "DE",
            "Delaware",
            "DC",
            "District of Columbia",
            "FM",
            "Federated States of Micronesia",
            "FL",
            "Florida",
            "GA",
            "Georgia",
            "GU",
            "Guam",
            "HI",
            "Hawaii",
            "ID",
            "Idaho",
            "IL",
            "Illinois",
            "IN",
            "Indiana",
            "IA",
            "Iowa",
            "KS",
            "Kansas",
            "KY",
            "Kentucky",
            "LA",
            "Louisiana",
            "ME",
            "Maine",
            "MH",
            "Marshall Islands",
            "MD",
            "Maryland",
            "MA",
            "Massachusetts",
            "MI",
            "Michigan",
            "MN",
            "Minnesota",
            "MS",
            "Mississippi",
            "MO",
            "Missouri",
            "MT",
            "Montana",
            "NE",
            "Nebraska",
            "NV",
            "Nevada",
            "NH",
            "New Hampshire",
            "NJ",
            "New Jersey",
            "NM",
            "New Mexico",
            "NY",
            "New York",
            "NC",
            "North Carolina",
            "ND",
            "North Dakota",
            "MP",
            "Northern Mariana Islands",
            "OH",
            "Ohio",
            "OK",
            "Oklahoma",
            "OR",
            "Oregon",
            "PW",
            "Palau",
            "PA",
            "Pennsylvania",
            "PR",
            "Puerto Rico",
            "RI",
            "Rhode Island",
            "SC",
            "South Carolina",
            "SD",
            "South Dakota",
            "TN",
            "Tennessee",
            "TX",
            "Texas",
            "UT",
            "Utah",
            "VT",
            "Vermont",
            "VI",
            "Virgin Islands",
            "VA",
            "Virginia",
            "WA",
            "Washington",
            "WV",
            "West Virginia",
            "WI",
            "Wisconsin",
            "WY",
            "Wyoming",
            "AA",
            "Armed Forces Americas",
            "AE",
            "Armed Forces Europe",
            "AP",
            "Armed Forces Pacific",
        ];

        for entry in valid_entries {
            assert!(prsr.re_state.is_match(entry), "Failed to match: {}", entry);
        }
    }

    #[test]
    fn test_regex_state_invalid() {
        let prsr = Prsr::new();

        let invalid_entries = vec![
            "InvalidState",
            "Cali",
            "New Y",
            "Tex",
            "ZZ",
            "A",
            "123",
            "Carolina",
        ];

        for entry in invalid_entries {
            assert!(
                !prsr.re_state.is_match(entry),
                "Incorrectly matched: {}",
                entry
            );
        }
    }

    #[test]
    fn test_regex_flt_valid() {
        let prsr = Prsr::new();

        let valid_cases = vec![
            "123.456",  // Positive decimal
            "-123.456", // Negative decimal
            "0.123",    // Positive decimal less than 1
            "-0.123",   // Negative decimal less than 1
            "10.0",     // Whole number as decimal
            "-10.0",    // Negative whole number as decimal
        ];

        for case in valid_cases {
            assert!(prsr.re_flt.is_match(case), "Failed to match: {}", case);
        }
    }

    #[test]
    fn test_regex_flt_invalid() {
        let prsr = Prsr::new();

        let invalid_cases = vec![
            "123",            // Integer
            "-123",           // Negative integer
            "123.",           // Decimal without fractional part
            ".456",           // Decimal without integer part
            "123.456.789",    // Multiple decimal points
            "123a.456",       // Alphabets in number
            "202-225-4735",   // Phone number with hyphens
            "(202) 225-4735", // Phone number with parentheses and spaces
            "12345",          // Zip code
            "12345-6789",     // Extended zip code
            "123 Main St",    // Address line
            "PO BOX 123",     // Address line with PO BOX
        ];

        for case in invalid_cases {
            assert!(!prsr.re_flt.is_match(case), "Incorrectly matched: {}", case);
        }
    }

    #[test]
    fn test_contains_time_valid() {
        let valid_cases = vec![
            "Lunch at 12 p.m.",
            "EVERY 1ST, 3RD, AND 5TH WED 12-4PM",
            "Meeting at 9AM.",
            "Dinner at 5PM today.",
            "4 a.m. is wakey time.",
            "See you at 8 am.",
            "Wake up at 6 pm.",
            "11 PM is sleepy time.",
            "Event at 3 A.M.",
            "Appointment at 7 P.M.",
        ];

        for case in valid_cases {
            assert!(
                contains_time(case),
                "Failed to match valid time in: {}",
                case
            );
        }
    }

    #[test]
    fn test_contains_time_invalid() {
        let invalid_cases = vec![
            "This is a test line.",
            "No time here.",
            "The meeting is at noon.",
            "It happened in the afternoon.",
            "Event at 17:00.",
            "Time format 24-hour 18:30.",
            // "Random text 9AMS.",
            // "5PMs is not a valid format.",
            "Midnight is at 00:00.",
        ];

        for case in invalid_cases {
            assert!(
                !contains_time(case),
                "Incorrectly matched invalid time in: {}",
                case
            );
        }
    }

    #[test]
    fn test_remove_initials() {
        let prsr = Prsr::new();
        assert_eq!(prsr.remove_initials("MICKEY J. MOUSE"), "MICKEY MOUSE");
        assert_eq!(prsr.remove_initials("JOHN R. SMITH"), "JOHN SMITH");
        assert_eq!(prsr.remove_initials("B. ALICE WALKER"), "ALICE WALKER");
        assert_eq!(prsr.remove_initials("A. B. C. D."), "D."); // Test with multiple initials
        assert_eq!(prsr.remove_initials("J. K. ROWLING"), "ROWLING"); // Test with multiple initials in sequence
    }

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

    #[test]
    fn test_trim_end_spc_pnc_valid() {
        let mut cases = [
            ("Hello, world!!!   ", "Hello, world"),
            ("No spaces here!", "No spaces here"),
            ("Just some spaces    ", "Just some spaces"),
            ("Punctuation...!!!", "Punctuation"),
            ("Whitespace \t\n", "Whitespace"),
            ("Mixed!!! \t\n...!!!", "Mixed"),
        ];

        for (input, expected) in cases.iter_mut() {
            let mut input_string = input.to_string();
            trim_end_spc_pnc(&mut input_string);
            assert_eq!(
                input_string,
                expected.to_string(),
                "Failed on input: '{}'",
                input
            );
        }
    }

    #[test]
    fn test_trim_end_spc_pnc_empty() {
        let mut input = "".to_string();
        let expected = "";
        trim_end_spc_pnc(&mut input);
        assert_eq!(input, expected);
    }

    #[test]
    fn test_trim_end_spc_pnc_no_trimming_needed() {
        let mut input = "Already trimmed".to_string();
        let expected = "Already trimmed";
        trim_end_spc_pnc(&mut input);
        assert_eq!(input, expected);
    }

    #[test]
    fn test_trim_end_spc_pnc_only_whitespace_and_punctuation() {
        let mut input = "!!! \t\n ...".to_string();
        let expected = "";
        trim_end_spc_pnc(&mut input);
        assert_eq!(input, expected);
    }

    #[test]
    fn test_trim_end_spc_pnc_invalid_cases() {
        let mut cases = [
            ("   leading spaces", "   leading spaces"),
            ("middle spaces  here", "middle spaces  here"),
            (
                "punctuation in the middle...here",
                "punctuation in the middle...here",
            ),
        ];

        for (input, expected) in cases.iter_mut() {
            let mut input_string = input.to_string();
            trim_end_spc_pnc(&mut input_string);
            assert_eq!(
                input_string,
                expected.to_string(),
                "Failed on input: '{}'",
                input
            );
        }
    }
}
