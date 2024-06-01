use crate::models::*;
use crate::usps::*;
use anyhow::{anyhow, Result};
use regex::Regex;

pub struct Prsr {
    /// A regex matching a floating point number.
    // "46.86551919465073", "-96.83144324414937"
    pub re_flt: Regex,
    /// A regex matching initials in a name.
    pub re_name_initials: Regex,
}

impl Prsr {
    pub fn new() -> Self {
        Prsr {
            re_flt: Regex::new(r"^-?\d+\.\d+$").unwrap(),
            re_name_initials: Regex::new(r"\b[A-Z]\.\s+").unwrap(),
        }
    }

    pub fn filter(&self, s: &str) -> bool {
        !s.is_empty()
            && !s.contains("IFRAME")
            && !s.contains("FUNCTION")
            && !s.contains("FORM")
            && !s.contains("!IMPORTANT;")
            && !s.starts_with('(')
            && !s.contains("PHONE:")
            && !s.contains("FAX:")
            && !(s.contains("AM") && s.contains("PM") && s.contains("TO"))
            && !self.re_flt.is_match(s)
    }

    pub fn edit_lnes(&self, lnes: &mut Vec<String>) {
        // Edit lines to make it easier to parse.
        edit_split_bar(lnes);
        edit_split_city_state_zip(lnes);
        edit_disjoint_zip(lnes);
        edit_drain_after_last_zip(lnes);
        edit_dot(lnes);
    }

    pub fn remove_initials(&self, full_name: &str) -> String {
        // Define the regular expression to match initials
        let re = Regex::new(r"\b[A-Z]\.\s+").unwrap();
        // Replace the initials with an empty string
        self.re_name_initials.replace_all(full_name, "").to_string()
    }
}

pub fn parse_addresses(per: &Person, lnes: &[String]) -> Option<Vec<Address>> {
    // eprintln!("--- parse_addresses: {lnes:?}");

    // Start from the bottom.
    // Search for a five digit zip code.
    let mut adrs: Vec<Address> = Vec::new();
    const LEN_ZIP: usize = 5;
    for (idx, lne) in lnes.iter().enumerate().rev() {
        if lne.len() == LEN_ZIP && ends_with_5digits(lne) {
            // eprintln!("-- parse_addresses: idx:{idx}");
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
                // Check for edge cases:
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
            adrs.push(adr);
        }
    }

    // Deduplicate extracted addresses.
    adrs.sort_unstable();
    adrs.dedup_by(|a, b| a == b);
    // adrs.reverse();
    // adrs.retain(|v| v.city == "DAYTON");
    // adrs[0].zip = "77535".into();

    eprintln!("{} addresses parsed.", adrs.len());

    if adrs.is_empty() {
        return None;
    }

    Some(adrs)
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

pub async fn standardize_addresses(
    adrs: &mut Vec<Address>,
    cli_usps: &mut UspsClient,
) -> Result<()> {
    for idx in (0..adrs.len()).rev() {
        match cli_usps.standardize_address(&mut adrs[idx]).await {
            Ok(_) => {
                // Edit edge cases:
                // 2743 PERIMETR PKWY BLDG 200 STE 105,STE 105,AUGUSTA,GA
                if let Some(idx_fnd) = adrs[idx].address1.find("BLDG") {
                    adrs[idx].address2 = Some(adrs[idx].address1[idx_fnd..].to_string());
                    adrs[idx].address1.truncate(idx_fnd - 1); // truncate extra space
                }
                // 685 CARNEGIE DR STE 100,,SAN BERNARDINO,CA
                else if let Some(idx_fnd) = adrs[idx].address1.find("STE") {
                    adrs[idx].address2 = Some(adrs[idx].address1[idx_fnd..].to_string());
                    adrs[idx].address1.truncate(idx_fnd - 1); // truncate extra space
                }
                // 1070 MAIN ST UNIT 300,,PAWTUCKET,RI
                else if let Some(idx_fnd) = adrs[idx].address1.find("UNIT") {
                    adrs[idx].address2 = Some(adrs[idx].address1[idx_fnd..].to_string());
                    adrs[idx].address1.truncate(idx_fnd - 1); // truncate extra space
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

pub fn edit_title_military(lnes: &mut [String]) {
    // Remove dots.
    // "DR. WILLIAM" -> "WILLIAM"
    // "GENERAL CHARLES" -> "CHARLES"
    // "ADMIRAL CHRISTOPHER" -> "CHRISTOPHER"
    for lne in lnes.iter_mut() {
        if lne.starts_with("DR. ") {
            *lne = lne.replace("DR. ", "");
        } else if lne.starts_with("GENERAL ") {
            *lne = lne.replace("GENERAL ", "");
        } else if lne.starts_with("ADMIRAL ") {
            *lne = lne.replace("ADMIRAL ", "");
        }
    }
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

#[cfg(test)]
mod tests {
    use super::*;

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
}
