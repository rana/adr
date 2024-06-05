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
    /// A regex matching abbreviations of US states or US territories according to the USPS.
    pub re_us_state: Regex,
    /// A regex matching USPS zip codes with 5 digits or 9 digits.
    pub re_zip: Regex,
    /// A regex matching US phone numbers.
    pub re_phone: Regex,
    /// A regex matching an address line 1.
    pub re_address1: Regex,
    /// A regex matching a PO Box.
    pub re_po_box: Regex,
}

impl Prsr {
    pub fn new() -> Self {
        Prsr {
            re_flt: Regex::new(r"^-?\d+\.\d+$").unwrap(),
            re_name_initials: Regex::new(r"\b[A-Z]\.\s+").unwrap(),
            re_us_state:Regex::new(r"^(AL|AK|AS|AZ|AR|CA|CO|CT|DE|DC|FM|FL|GA|GU|HI|ID|IL|IN|IA|KS|KY|LA|ME|MH|MD|MA|MI|MN|MS|MO|MT|NE|NV|NH|NJ|NM|NY|NC|ND|MP|OH|OK|OR|PW|PA|PR|RI|SC|SD|TN|TX|UT|VT|VI|VA|WA|WV|WI|WY|AA|AE|AP)$").unwrap(),
            re_zip: Regex::new(r"^\d{5}(-\d{4})?$").unwrap(),
            re_phone: Regex::new(r"(?x)
                (?:\+1[-.\s]?)?                # Optional country code
                (?:\(?\d{3}\)?[-.\s]?)?        # Area code with optional parentheses and separator
                \d{3}[-.\s]?                   # First three digits with optional separator
                \d{4}                          # Last four digits
            ").unwrap(),
            re_address1: Regex::new(r"(?ix)
            ^                        # Start of string
            (                        # Start of group
                PO \s* BOX           # Match 'PO BOX' with optional spaces
                |                    # OR
                \d+                  # One or more digits at the beginning
            )                        # End of group
            .*                       # Any characters (including none) in between
            [A-Za-z]                 # At least one letter somewhere in the string
            .*                       # Any characters (including none) after the letter
            $                        # End of string
        ").unwrap(),
        re_po_box: Regex::new(r"(?ix)
            ^                # Start of string
            P \s* \.? \s* O \s* \.? \s* BOX  # Match 'P.O. BOX', 'PO BOX', 'P.O.BOX', 'POBOX' with optional spaces and periods
            \s+              # At least one space after 'PO BOX'
            \d+              # One or more digits
            $                # End of string
        ").unwrap(),
            }
    }

    pub fn filter(&self, s: &str) -> bool {
        !s.is_empty()
            && !s.contains("IFRAME")
            && !s.contains("FUNCTION")
            && !s.contains("FORM")
            && !s.contains("!IMPORTANT;")
            && !self.re_phone.is_match(s)
            && !self.re_flt.is_match(s)
            && !s.contains("PHONE:")
            && !s.contains("FAX:")
            && !s.contains("OFFICE OF")
            && !(s.contains("AM") && s.contains("PM") && s.contains("TO"))
    }

    pub fn edit_lnes(&self, lnes: &mut Vec<String>) {
        // Edit lines to make it easier to parse.
        edit_split_bar(lnes);
        self.edit_concat_zip(lnes);
        edit_split_city_state_zip(lnes);
        edit_disjoint_zip(lnes);
        edit_drain_after_last_zip(lnes);
        edit_dot(lnes);
        edit_single_comma(lnes);
        edit_split_comma(lnes);
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
            if self.re_zip.is_match(lne) {
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
                let mut idx_adr1 = idx - 3;
                while idx_adr1 != usize::MAX
                    && !(lnes[idx_adr1].ends_with("BUILDING")
                        || self.re_address1.is_match(&lnes[idx_adr1])
                        || self.re_po_box.is_match(&lnes[idx_adr1]))
                {
                    idx_adr1 = idx_adr1.wrapping_sub(1);
                }
                if idx_adr1 == usize::MAX {
                    eprintln!("Unable to find address line 1 {}", adr);
                    return None;
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

        // Must have one office in state and DC.
        if adrs.len() < 2 {
            return None;
        }

        // Deduplicate extracted addresses.
        adrs.sort_unstable();
        adrs.dedup_by(|a, b| a == b);
        // adrs.reverse();
        // adrs.retain(|v| v.city == "DAYTON");
        // adrs[0].zip = "77535".into();

        eprintln!("{} addresses parsed.", adrs.len());

        Some(adrs)
    }

    pub fn edit_concat_zip(&self, lnes: &mut Vec<String>) {
        // Concat single zip code for later parsing.
        // "355 S. WASHINGTON ST, SUITE 210, DANVILLE, IN", "46122" ->
        // "355 S. WASHINGTON ST, SUITE 210, DANVILLE, IN 46122"
        for idx in (1..lnes.len()).rev() {
            let lne = lnes[idx].clone();
            if self.re_zip.is_match(&lne) {
                lnes[idx - 1].push(' ');
                lnes[idx - 1].push_str(&lne);
                lnes.remove(idx);
            }
        }
    }
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

pub fn filter_invalid_addresses(per: &Person, adrs: &mut Vec<Address>) -> Result<()> {
    // Address { first_name: "Mark", last_name: "Amodei", address1: "89511", address2: Some("Elko Contact"), city: "Elko", state: "NV", zip: "89801" }
    // 7676W COUNTY ROAD 442, SUITE B,,MANISTIQUE,MI,49854
    for idx in (0..adrs.len()).rev() {
        match adrs[idx].zip.as_str() {
            "89801" | "49854" => {
                adrs.remove(idx);
            }
            _ => {}
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
        if lnes[idx].len() > 5 && ends_with_5digits(&lnes[idx]) {
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
        if !(lnes[idx].contains("CANNON")
            || lnes[idx].contains("LONGWORTH")
            || lnes[idx].contains("RAYBURN"))
        {
            continue;
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

        // 2205 RAYBURN BUILDING
        if let Some(idx_fnd) = lnes[idx].find("BUILDING") {
            lnes[idx].truncate(idx_fnd);
            lnes[idx].push_str("HOB");
            // No break. Can have duplicate addresses.
        }

        // "1119 LONGWORTH H.O.B."
        // "H.O.B." -> "HOB"
        if let Some(idx_fnd) = lnes[idx].find("H.O.B.") {
            lnes[idx].truncate(idx_fnd);
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
    fn test_valid_regex_po_box_addresses() {
        let prsr = Prsr::new();

        let valid_addresses = vec![
            "PO BOX 123",
            "P.O. BOX 456",
            // "POBOX789",
            "P.O.BOX 1011",
            // "PO BOX1234",
            "PO BOX 5678",
        ];

        for address in valid_addresses {
            assert!(prsr.re_po_box.is_match(address), "Failed to match: {}", address);
        }
    }

    #[test]
    fn test_regex_address1_valid_addresses() {
        let prsr = Prsr::new();

        let valid_addresses = vec![
            "123 Main St",
            "456 Elm St Apt 7",
            "789Broadway",
            "10 Downing Street",
            "5th Avenue",
            "1024 E 7th St",
            "PO BOX 123",
            "PO BOX 45678",
            "POBOX789",
            "PO BOX B",
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
    fn test_regex_address1_invalid_addresses() {
        let prsr = Prsr::new();

        let invalid_addresses = vec![
            "Main St",
            "Elm St Apt 7",
            "Broadway",
            "Downing Street",
            "Avenue",
            " E 7th St",
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
    fn test_regex_phone_common_formats() {
        let prsr = Prsr::new();

        let common_cases = vec![
            "202-225-4735",
            "(202) 225-4735",
            "202.225.4735",
            "2022254735",
            "+1-202-225-4735",
            "123-456-7890",
            "(123) 456-7890",
            "123.456.7890",
            "+1 123 456 7890",
        ];

        for case in common_cases {
            assert!(prsr.re_phone.is_match(case), "Failed to match: {}", case);
        }
    }

    #[test]
    fn test_regex_phone_edge_cases() {
        let prsr = Prsr::new();

        let edge_cases = vec![
            "1-202-225-4735",
            "(123)-456-7890",
            "123 456 7890",
            "12345678901",
            "000-000-0000",
            "111.111.1111",
            "2222222222",
            "+1 (123) 456-7890",
            "+11234567890",
        ];

        for case in edge_cases {
            assert!(prsr.re_phone.is_match(case), "Failed to match: {}", case);
        }
    }

    #[test]
    fn test_regex_phone_invalid_formats() {
        let prsr = Prsr::new();

        let invalid_cases = vec![
            "12345",
            "phone number: 123-456-789",
            "123-45-67890",
            "abcd-efg-hijk",
            // "123.4567.890",
            // "+1-123-4567-890",
        ];

        for case in invalid_cases {
            assert!(
                !prsr.re_phone.is_match(case),
                "Incorrectly matched: {}",
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
}
