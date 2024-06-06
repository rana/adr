use std::char;

use crate::models::*;
use crate::safe_slice_from_end;
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
    pub re_us_state: Regex,
    /// A regex matching US phone numbers.
    pub re_phone: Regex,
    /// A regex matching an address line 1.
    pub re_address1: Regex,
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
            re_us_state:Regex::new(r"^(AL|AK|AS|AZ|AR|CA|CO|CT|DE|DC|FM|FL|GA|GU|HI|ID|IL|IN|IA|KS|KY|LA|ME|MH|MD|MA|MI|MN|MS|MO|MT|NE|NV|NH|NJ|NM|NY|NC|ND|MP|OH|OK|OR|PW|PA|PR|RI|SC|SD|TN|TX|UT|VT|VI|VA|WA|WV|WI|WY|AA|AE|AP)$").unwrap(),
            re_phone: Regex::new(r"(?x)
                ^                        # Start of string
                (?:\+1[-.\s]?)?          # Optional country code
                (?:\(?\d{3}\)?[-.\s])    # Area code with optional parentheses and required separator
                \d{3}[-.\s]?             # First three digits with optional separator
                \d{4}                    # Last four digits
                $                        # End of string
            ").unwrap(),
            re_address1: Regex::new(r"(?x)
                ^                # Start of string
                \d+              # One or more digits at the beginning
                [-]?             # Zero or one minus sign.
                \d+              # One or more digits
                [A-Za-z]?        # Zero or one letter after the digits
                \s+              # One or more spaces after the digits
                .*               # Any characters (including none) in between
                [A-Za-z]         # At least one letter somewhere in the string
                .*               # Any characters (including none) after the letter
                $                # End of string
            ").unwrap(),
            re_po_box: Regex::new(r"(?ix)
                ^                # Start of string
                P \s* \.? \s* O \s* \.? \s* BOX  # Match 'P.O. BOX', 'PO BOX', 'P.O.BOX', 'POBOX' with optional spaces and periods
                \s+              # At least one space after 'PO BOX'
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
            && !self.re_phone.is_match(s)
            && !self.re_flt.is_match(s)
            && !s.contains("PHONE:")
            && !s.contains("FAX:")
            && !s.contains("OFFICE OF")
            && !contains_time(s)
    }

    pub fn edit_lnes(&self, lnes: &mut Vec<String>) {
        // Edit lines to make it easier to parse.
        edit_split_bar(lnes);
        self.edit_concat_zip(lnes);
        edit_split_city_state_zip(lnes);
        edit_zip_disjoint(lnes);
        // edit_drain_after_last_zip(lnes);
        edit_dot(lnes);
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
            if is_zip(lne) {
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
                let mut idx_adr1 = idx - 3;
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
                if idx_adr1 != 0 && self.re_address1.is_match(&lnes[idx_adr1 - 1]) {
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

        // Must have one office in state and DC.
        if adrs.len() < 2 {
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
            if is_zip(&lne) && !self.re_us_state.is_match(&lnes[idx - 1]) {
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
    // 176 MUNICIPAL WAY,,SANTEE,SC,29142
    for idx in (0..adrs.len()).rev() {
        match adrs[idx].zip.as_str() {
            "89801" | "49854" | "78702" | "29142" | "85139" | "78071" | "07410" => {
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
    // Invalid split: "P.O. BOX 9023958", "SAN JUAN", "PR", "00902-3958"
    for idx in (0..lnes.len()).rev() {
        if !is_zip(&lnes[idx]) && ends_with_zip(&lnes[idx]) {
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

pub fn edit_drain_after_last_zip(lnes: &mut Vec<String>) {
    // Trim the list after the last zip code.
    // Search for the last zip code.
    for idx in (0..lnes.len()).rev() {
        if ends_with_zip(&lnes[idx]) {
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

        // TODO: DELETE AND CHECK
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

pub fn edit_by_appt(lnes: &mut [String]) {
    // Remove "(BY APPT ONLY)".
    // "10167 SOCORRO RD (BY APPT ONLY)" -> "10167 SOCORRO RD"
    for lne in lnes.iter_mut() {
        if let Some(idx_fnd) = lne.find("(BY APPT ONLY)") {
            *lne = lne[..idx_fnd].trim().to_string();
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

/// Checks whether a string ends with a USPS zip with 5 digits or 9 digits.
pub fn ends_with_zip(lne: &str) -> bool {
    // Check 5 digit zip.
    if lne.len() < LEN_ZIP5 {
        return false;
    }

    if is_zip(safe_slice_from_end(lne, LEN_ZIP5)) {
        if lne.len() == LEN_ZIP5 {
            return true;
        }

        // Edge case checks.
        // Check for too many digits, 123456.
        // Check for invalid zip, 12345-67890.
        if let Some(ch) = lne.chars().rev().nth(LEN_ZIP5) {
            if !ch.is_ascii_digit() && ch != ZIP_DASH {
                return true;
            }
        }
    }

    // Check 10 digit zip.
    if lne.len() < LEN_ZIP10 {
        return false;
    }
    is_zip(safe_slice_from_end(lne, LEN_ZIP10))
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_regex_po_box_valid() {
        let prsr = Prsr::new();

        let valid_addresses = vec![
            "PO BOX 123",
            "P.O. BOX 456",
            // "POBOX789",
            "P.O.BOX 1011",
            // "PO BOX1234",
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
    fn test_ends_with_zip_valid() {
        let valid_cases = vec![
            "This is a sentence ending with a zip 12345",
            "Another sentence ending with extended zip 12345-6789",
            "Just a zip code 54321",
            "Zip code at end 98765-4321",
        ];

        for case in valid_cases {
            assert!(
                ends_with_zip(case),
                "Failed to match valid zip code: {}",
                case
            );
        }
    }

    #[test]
    fn test_ends_with_zip_invalid() {
        let invalid_cases = vec![
            "This has no zip code",
            "1234",        // Less than five digits
            "123456",      // More than five digits without hyphen
            "1234-5678",   // Less than five digits before hyphen
            "12345-678",   // Less than four digits after hyphen
            "12345-67890", // More than four digits after hyphen
            "12345 6789",  // Space instead of hyphen
            "12a45-6789",  // Alphabetic character in zip code
            "12345-678a",  // Alphabetic character in extended part
            "Sentence with 12345 in the middle",
            "P.O. BOX 9023958",
        ];

        for case in invalid_cases {
            assert!(
                !ends_with_zip(case),
                "Incorrectly matched invalid zip code: {}",
                case
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
}
