use crate::models::*;
use crate::usps::*;
use anyhow::{anyhow, Result};
use regex::Regex;
use std::char;
use std::clone;

lazy_static! {
    pub static ref PRSR: Prsr = Prsr::new();
}

pub struct Prsr {
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
    /// A regex matching parentheses.
    pub re_parens: Regex,
    /// A regex matching a floating point number:
    /// "46.86551919465073", "-96.83144324414937".
    pub re_flt: Regex,
    /// A regex matching initials in a name.
    pub re_name_initials: Regex,
    /// A regex matching name affectations.
    pub re_name_affectation: Regex,
}

impl Prsr {
    pub fn new() -> Self {
        Prsr {
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
                    #|                # OR
                    #NASA             # NASA
                    #['s]*            
                )
                \s+              # One or more spaces after the digits
                .*               # Any characters (including none) in between
                [A-Za-z]         # At least one letter somewhere in the string
                .*               # Any characters (including none) after the letter
                |                # OR
                CENTER           # For 'SPACE CENTER'
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
            re_parens: Regex::new(r"\(.*?\)").unwrap(),
            re_flt: Regex::new(r"^-?\d+\.\d+$").unwrap(),
            re_name_initials: Regex::new(r"\b[A-Z]\.\s+").unwrap(), // Allow: A.C. Quincy, r"\b[A-Z]\.([A-Z]\.)*\s+"
            re_name_affectation: Regex::new(r#"(?xi)
                (
                    "[^"]*"         # Quoted text
                    |               # OR
                    \(.*?\)         # Text in parentheses
                    |               # OR
                    Gov\.           # 'Gov.' abbreviation
                    |               # OR
                    Jr\.           # 'Jr.' abbreviation
                    |               # OR
                    Dr\.            # 'Dr.' abbreviation
                    |               # OR
                    Dr\b            # 'Dr' abbreviation
                    |                   # OR
                    (?:Ph|Ed)\.?\s*D\.  # 'Ph.D.', 'Ph. D.', 'Ph D.' abbreviation
                    |                   # OR
                    (?:Ph|Ed)\s*D\b     # 'PhD', 'Ph D' abbreviation
                    |                   # OR
                    J\.?\s*D\.          # 'J.D.', 'J. D.' abbreviation
                    |                   # OR
                    JD\b                # 'JD' abbreviation
                    |                   # OR
                    MPH\b               # 'MPH' abbreviation
                    |                   # OR
                    CIH\b               # 'CIH' abbreviation
                    |                   # OR
                    (II|III|IV)\b       # Roman numerals
                    \b                  # Word boundry
                )
            "#).unwrap(), 
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
            && !s.contains("ELEMENTOR")
            && !s.contains("DIRECTIONS")
            && !s.contains("ENTRANCE")
            && !self.re_phone.is_match(s)
            && !self.re_flt.is_match(s)
            && !s.contains("PHONE")
            // && !s.contains("FAX") // Invalid case: FAIRFAX
            // && !s.contains("OFFICE OF") // Invalid case: "OFFICE OF GOVERNOR PO BOX 001"
            && !s.starts_with("P: ")
            && !s.starts_with("F: ")
            && !s.starts_with("MAIN:")
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
        // eprintln!("(5) {lnes:?}");
        edit_single_comma(lnes);
        edit_zip_20003(lnes);
    }

    pub fn prs_adrs(&self, lnes: &[String]) -> Option<Vec<Address>> {
        // eprintln!("--- parse_addresses: {lnes:?}");

        // Start from the bottom.
        // Search for a five digit zip code.
        let mut adrs: Vec<Address> = Vec::new();
        for (idx, lne) in lnes.iter().enumerate().rev() {
            let is_zip5 = is_zip5(lne);
            let is_zip10 = if !is_zip5 { is_zip10(lne) } else { false };
            if (is_zip5 || is_zip10) && !is_invalid_zip(lne) {
                // eprintln!("-- parse_addresses: idx:{idx}");
                // Start of an address.
                let mut adr = Address::default();
                if is_zip5 {
                    adr.zip5 = lne.parse().unwrap();
                } else {
                    adr.zip5 = lne[..5].parse().unwrap();
                    adr.zip4 = lne[lne.len() - 4..].parse().unwrap();
                }
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

    pub fn two_zip_or_more(&self, lnes: &[String]) -> bool {
        lnes.iter().filter(|lne| is_zip(lne)).count() >= 2
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
                } else {
                    // Check if street and city not delimited.
                    // 615 E WORTHY STREET GONZALES
                    // 430 NORTH FRANKLIN ST FORT BRAGG, CA 95437
                    // "GLEN ALLEN, VA 23060"
                    // "SAN LUIS OBISPO, CA 93401"
                    lnes.insert(idx, lne);
                }
            }
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

pub fn edit_sob(lnes: &mut Vec<String>) {
    // Trim list prefix prior to "Senate Office Building"
    // Reverse indexes to allow for room line removal.
    for idx in (0..lnes.len()).rev() {
        if lnes[idx].starts_with("2 CONSTITUTION AVE")
            || lnes[idx].starts_with("50 CONSTITUTION AVE")
            || lnes[idx].starts_with("120 CONSTITUTION AVE")
        {
            lnes.remove(idx);
        }

        const HART: &str = "HART";
        const DIRKSEN: &str = "DIRKSEN";
        const RUSSELL: &str = "RUSSELL";
        if !(lnes[idx].contains(HART) || lnes[idx].contains(DIRKSEN) || lnes[idx].contains(RUSSELL))
        {
            continue;
        }

        // "509 HART", "SENATE OFFICE BLDG"
        if idx + 1 != lnes.len()
            && (lnes[idx].ends_with(HART)
                || lnes[idx].ends_with(DIRKSEN)
                || lnes[idx].ends_with(RUSSELL))
            && lnes[idx + 1].starts_with("SENATE OFFICE")
        {
            lnes[idx].push_str(" SOB");
            lnes.remove(idx + 1);
        }

        // "110 HART SENATE OFFICE", "BUILDING"
        if idx + 1 != lnes.len()
            && lnes[idx].ends_with("SENATE OFFICE")
            && lnes[idx + 1] == "BUILDING"
        {
            lnes[idx] = lnes[idx].replace("SENATE OFFICE", "SOB");
            lnes.remove(idx + 1);
        }

        // "502 HART SENATE OFFICE BUILDING"
        if let Some(idx_fnd) = lnes[idx].find("SENATE OFFICE") {
            lnes[idx].truncate(idx_fnd);
            lnes[idx].push_str("SOB");
        }

        // "313 HART OFFICE BUILDING"
        if let Some(idx_fnd) = lnes[idx].find("OFFICE BUILDING") {
            lnes[idx].truncate(idx_fnd);
            lnes[idx].push_str("SOB");
        }

        // "331 HART SENATE", "OFFICE BUILDING"
        // "503 HART SENATE", "OFFICE BLDG."
        if lnes[idx].ends_with("SENATE") && lnes[idx + 1].starts_with("OFFICE") {
            lnes[idx] = lnes[idx].replace("SENATE", "SOB");
            lnes.remove(idx + 1);
        }

        // 261 RUSSELL SENATE BUILDING
        if let Some(idx_fnd) = lnes[idx].find("SENATE BUILDING") {
            lnes[idx].truncate(idx_fnd);
            lnes[idx].push_str("SOB");
        }

        // 133 HART BUILDING
        if let Some(idx_fnd) = lnes[idx].find("BUILDING") {
            lnes[idx].truncate(idx_fnd);
            lnes[idx].push_str("SOB");
        }

        // "ROOM 521"
        // "SUITE 455"
        // "SUITE SR-374"
        // "SUITE 479A"
        if idx + 1 != lnes.len()
            && (lnes[idx + 1].contains("ROOM") || lnes[idx + 1].contains("SUITE"))
            && lnes[idx].trim().ends_with("SOB")
        {
            // Filter digits.
            let mut adr1: String = lnes[idx + 1]
                .chars()
                .filter(|c| c.is_ascii_digit())
                .collect();
            adr1.push(' ');
            adr1.push_str(&lnes[idx]);
            lnes[idx] = adr1;
            lnes.remove(idx + 1);
        }

        if lnes[idx].contains(HART) {
            lnes[idx] = lnes[idx].replace("HART SOB", "HSOB");
        } else if lnes[idx].contains(DIRKSEN) {
            lnes[idx] = lnes[idx].replace("DIRKSEN SOB", "DSOB");
        } else if lnes[idx].contains(RUSSELL) {
            lnes[idx] = lnes[idx].replace("RUSSELL SOB", "RSOB");
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

        const CANNON: &str = "CANNON";
        const LONGWORTH: &str = "LONGWORTH";
        const RAYBURN: &str = "RAYBURN";
        if !(lnes[idx].contains(CANNON)
            || lnes[idx].contains(LONGWORTH)
            || lnes[idx].contains(RAYBURN))
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
        }

        // 2205 RAYBURN OFFICE BUILDING
        if let Some(idx_fnd) = lnes[idx].find("OFFICE BUILDING") {
            lnes[idx].truncate(idx_fnd);
            lnes[idx].push_str("HOB");
        }

        // 2205 RAYBURN BUILDING
        if let Some(idx_fnd) = lnes[idx].find("BUILDING") {
            lnes[idx].truncate(idx_fnd);
            lnes[idx].push_str("HOB");
        }

        // "LONGWORTH HOB", "ROOM 1027"
        if idx + 1 != lnes.len()
            && lnes[idx + 1].contains("ROOM")
            && lnes[idx].trim().ends_with("HOB")
        {
            let room: Vec<&str> = lnes[idx + 1].split_whitespace().collect();
            lnes[idx] = format!("{} {}", room[1], lnes[idx]);
            lnes.remove(idx + 1);
        }

        if lnes[idx].contains(CANNON) {
            lnes[idx] = lnes[idx].replace("CANNON HOB", "CHOB");
        } else if lnes[idx].contains(LONGWORTH) {
            lnes[idx] = lnes[idx].replace("LONGWORTH HOB", "LHOB");
        } else if lnes[idx].contains(RAYBURN) {
            lnes[idx] = lnes[idx].replace("RAYBURN HOB", "RHOB");
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

pub fn edit_zip_20003(lnes: &mut [String]) {
    // Change DC zip code.
    // 143 CHOB,,WASHINGTON,DC,20003
    for idx in (0..lnes.len()).rev() {
        if lnes[idx] == "20003" {
            lnes[idx] = "20515".into();
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

pub fn edit_starting_hash(lnes: &mut [String]) {
    // Remove (#).
    // "#3 TENNESSEE AVENUE" -> "3 TENNESSEE AVENUE"
    // Invalid case: "1000 GLENN HEARN BOULEVARD", "#20127"
    for lne in lnes.iter_mut() {
        if lne.starts_with('#') && lne.len() > 1 && !lne.chars().skip(1).all(|c| c.is_ascii_digit())
        {
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

pub fn edit_nbsp_zwsp(lnes: &mut [String]) {
    const NBSP: char = '\u{a0}'; // non-breaking space
    const ZWSP: char = '\u{200b}'; // zero-width space
    for idx in (0..lnes.len()).rev() {
        if lnes[idx].contains(NBSP) {
            lnes[idx] = lnes[idx]
                .chars()
                .map(|c| if c == NBSP { ' ' } else { c })
                .collect();
        }
        if lnes[idx].contains(ZWSP) {
            lnes[idx] = lnes[idx].chars().filter(|&c| c != ZWSP).collect();
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
            // Check for invalid cases.
            //  - Too many digits: "123456".
            //  - 10 char zip: "12345-67890".
            //  - Unit number: "#20127".
            //  - Room number: "ROOM 20100".
            //  - Suite number: "SUITE 20350".
            //  - Box number: "BOX 22201".
            const IDX_ROOM: usize = 10;
            if lne.len() >= IDX_ROOM && lne[lne.len() - IDX_ROOM..].starts_with("ROOM") {
                return None;
            }
            const IDX_SUITE: usize = 11;
            if lne.len() >= IDX_SUITE && lne[lne.len() - IDX_SUITE..].starts_with("SUITE") {
                return None;
            }
            const IDX_BOX: usize = 9;
            if lne.len() >= IDX_BOX && lne[lne.len() - IDX_BOX..].starts_with("BOX") {
                return None;
            }
            if let Some(c) = lne.chars().rev().nth(LEN_ZIP5) {
                if !c.is_ascii_digit() && c != ZIP_DASH && c != '#' {
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
    let chr_len = lne.chars().count();
    if chr_len > LEN_ZIP10 {
        // Check 10 digit zip.
        let zip: String = lne.chars().skip(chr_len - LEN_ZIP10).collect();
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

pub fn nbsp_replace(mut s: String) -> String {
    const NBSP: char = '\u{a0}'; // non-breaking space
    if s.contains(NBSP) {
        s = s.chars().map(|c| if c == NBSP { ' ' } else { c }).collect();
    }
    s
}

pub fn rht_quo_replace(mut s: String) -> String {
    const RHT_QUO: char = '\u{2019}'; // Right Single Quotation Mark
    if s.contains(RHT_QUO) {
        s = s
            .chars()
            .map(|c| if c == RHT_QUO { '\'' } else { c })
            .collect();
    }
    s
}

pub fn zwsp_remove(mut s: String) -> String {
    const ZWSP: char = '\u{200b}'; // zero-width space
    if s.contains(ZWSP) {
        s = s.chars().filter(|&c| c != ZWSP).collect();
    }

    s
}

pub fn dot_remove(mut s: String) -> String {
    const ZWSP: char = '.'; // dot
    if s.contains(ZWSP) {
        s = s.chars().filter(|&c| c != ZWSP).collect();
    }

    s
}

pub fn name_clean(full_name: &str) -> String {
    // Replace name affectations with an empty string
    let mut s = PRSR.re_name_affectation.replace_all(full_name, "");

    // Replace non-breaking space
    let mut s = nbsp_replace(s.to_string());

    // Replace right quote
    s = rht_quo_replace(s.to_string());

    // Trim
    s.trim().trim_end_matches(',').trim().replace("  ", " ")
}

pub fn name_clean_split(full_name: &str) -> (String, String) {
    // Support two-word last names.
    // "John Quincy Public"
    let full_name = name_clean(full_name);
    let names = full_name.split_once(' ').unwrap_or_default();
    (names.0.into(), names.1.into())
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
            "LANGLEY RESEARCH CENTER",
            "KENNEDY SPACE CENTER",
            "NASA Ames Research Center",
            "NASA's Johnson Space Center",
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
            "#20127",                           // Address unit number
            "123456",                           // Too many digits
            "Address with 1234",                // Less than 5 digits
            "Zip 1234-5678",                    // Invalid zip with too many digits after dash
            "Random text",                      // No zip code
            "45678-1234",                       // Valid 9-digit zip
            "Address with zip code 12345-6789", // Valid 9-digit zip
            "P.O. BOX 9023958",                 // PO box number
            "BOX 22201",                        // Box number
            "ROOM 20100",                       // Room number
            "SUITE 20350",                      // Suite number
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
            "P.O. BOX 9023958",        // PO box number
            "BOX 22201",               // Box number
            "ROOM 20100",              // Room number
            "SUITE 20350",             // Suite number
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
    fn test_regex_parens_valid() {
        let prsr = Prsr::new();

        // Valid cases
        let cases = vec![
            ("General Lester L. Lyles (USAF, Ret.)", vec!["(USAF, Ret.)"]),
            ("George A. Scott (acting)", vec!["(acting)"]),
            ("(start) middle (end)", vec!["(start)", "(end)"]),
            ("No text in (parentheses)", vec!["(parentheses)"]),
            ("Multiple (one) and (two)", vec!["(one)", "(two)"]),
        ];

        for (text, expected) in cases {
            let matches: Vec<&str> = prsr.re_parens.find_iter(text).map(|m| m.as_str()).collect();
            assert_eq!(matches, expected);
        }
    }

    #[test]
    fn test_regex_parens_invalid() {
        let prsr = Prsr::new();

        // Invalid cases
        let cases = vec![
            "No parentheses here",
            "Unmatched (parentheses",
            "Unmatched parentheses)",
            "Another example without",
        ];

        for text in cases {
            let matches: Vec<&str> = prsr.re_parens.find_iter(text).map(|m| m.as_str()).collect();
            assert!(matches.is_empty());
        }
    }

    #[test]
    fn test_regex_name_affectation_valid() {
        let prsr = Prsr::new();

        // Test quoted text
        let text1 = r#"This is a "test" for quoted "text"."#;
        let matches: Vec<&str> = prsr
            .re_name_affectation
            .find_iter(text1)
            .map(|m| m.as_str())
            .collect();
        assert_eq!(matches, vec![r#""test""#, r#""text""#]);

        // Test doctor abbreviation
        let text2 = "Dr. John, dr. Jane, and DR. Smith. Also Dr DR dr is fine.";
        let matches: Vec<&str> = prsr
            .re_name_affectation
            .find_iter(text2)
            .map(|m| m.as_str())
            .collect();
        assert_eq!(matches, vec!["Dr.", "dr.", "DR.", "Dr", "DR", "dr"]);

        // Test PhD abbreviation
        let text3 =
            "John has a PhD, jane has a phd, and SMITH has a PHD. Also Ph. D. and Ph.D. are fine.";
        let matches: Vec<&str> = prsr
            .re_name_affectation
            .find_iter(text3)
            .map(|m| m.as_str())
            .collect();
        assert_eq!(matches, vec!["PhD", "phd", "PHD.", "Ph. D.", "Ph.D."]);

        // Test EdD abbreviation
        let text3 =
            "John has a EdD, jane has a edd, and SMITH has a EDD. Also Ed. D. and Ed.D. are fine.";
        let matches: Vec<&str> = prsr
            .re_name_affectation
            .find_iter(text3)
            .map(|m| m.as_str())
            .collect();
        assert_eq!(matches, vec!["EdD", "edd", "EDD.", "Ed. D.", "Ed.D."]);

        // Test JD abbreviation
        let text2 = "John J.D., Jane J. D., and Smith JD. Also Jd, jd is fine.";
        let matches: Vec<&str> = prsr
            .re_name_affectation
            .find_iter(text2)
            .map(|m| m.as_str())
            .collect();
        assert_eq!(matches, vec!["J.D.", "J. D.", "JD.", "Jd", "jd"]);

        // Test MPH CIH abbreviation
        let text2 = "John MPH, Jane CIH are a suffixes.";
        let matches: Vec<&str> = prsr
            .re_name_affectation
            .find_iter(text2)
            .map(|m| m.as_str())
            .collect();
        assert_eq!(matches, vec!["MPH", "CIH"]);

        // Test Gov. Jr. II III IV
        let text2 = "Gov. John, Jr. II III IV are a suffixes.";
        let matches: Vec<&str> = prsr
            .re_name_affectation
            .find_iter(text2)
            .map(|m| m.as_str())
            .collect();
        assert_eq!(matches, vec!["Gov.", "Jr.", "II", "III", "IV"]);

        // Test text in parentheses
        let text2 = "Sometimes (like now) there are parentheses (here).";
        let matches: Vec<&str> = prsr
            .re_name_affectation
            .find_iter(text2)
            .map(|m| m.as_str())
            .collect();
        assert_eq!(matches, vec!["(like now)", "(here)"]);

        // Test combined text
        let text4 = r#""Quoted text" Dr. John Doe has a PhD in science."#;
        let matches: Vec<&str> = prsr
            .re_name_affectation
            .find_iter(text4)
            .map(|m| m.as_str())
            .collect();
        assert_eq!(matches, vec![r#""Quoted text""#, "Dr.", "PhD"]);
    }

    #[test]
    fn test_regex_name_affectation_invalid() {
        let prsr = Prsr::new();

        // Test string with no matches
        let text1 = "This string has no matches.";
        let matches: Vec<&str> = prsr
            .re_name_affectation
            .find_iter(text1)
            .map(|m| m.as_str())
            .collect();
        assert!(matches.is_empty());

        // Test string with partial matches
        let text3 = "A quotation mark \" is not a complete quoted text.";
        let matches: Vec<&str> = prsr
            .re_name_affectation
            .find_iter(text3)
            .map(|m| m.as_str())
            .collect();
        assert!(matches.is_empty());

        // Test string with name Eddie, Drew
        let text3 = "The names Eddie, Drew, PHDs, JDZZ, MPHZZ, CIHZZ aren't matches.";
        let matches: Vec<&str> = prsr
            .re_name_affectation
            .find_iter(text3)
            .map(|m| m.as_str())
            .collect();
        assert!(matches.is_empty());
    }

    #[test]
    fn test_regex_name_initials_valid() {
        assert_eq!(
            PRSR.re_name_initials.replace_all("MICKEY J. MOUSE", ""),
            "MICKEY MOUSE"
        );
        assert_eq!(
            PRSR.re_name_initials.replace_all("JOHN R. SMITH", ""),
            "JOHN SMITH"
        );
        assert_eq!(
            PRSR.re_name_initials.replace_all("B. ALICE WALKER", ""),
            "ALICE WALKER"
        );
        assert_eq!(PRSR.re_name_initials.replace_all("A. B. C. D.", ""), "D."); // Test with multiple initials
        assert_eq!(
            PRSR.re_name_initials.replace_all("J. K. ROWLING", ""),
            "ROWLING"
        ); // Test with multiple initials in sequence

        // Allow: A.C. Quincy
        // assert_eq!(
        //     PRSR.re_name_initials.replace_all("JOHN M.L. ADAMS", ""),
        //     "JOHN ADAMS"
        // );
    }

    #[test]
    fn test_regex_name_initials_invalid() {
        let prsr = Prsr::new();

        let text3 = "A.C. Quincy";
        let matches: Vec<&str> = prsr
            .re_name_initials
            .find_iter(text3)
            .map(|m| m.as_str())
            .collect();
        eprintln!("{matches:?}");
        assert!(matches.is_empty());
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
    fn test_nbsp_replace() {
        let cases = vec![
            ("Hello\u{a0}world", "Hello world"),
            ("\u{a0}Leading and trailing\u{a0}", " Leading and trailing "),
            ("No\u{a0}break\u{a0}spaces", "No break spaces"),
            ("Regular spaces only", "Regular spaces only"),
            ("", ""),
        ];

        for (input, expected) in cases {
            assert_eq!(nbsp_replace(input.to_string()), expected.to_string());
        }
    }

    #[test]
    fn test_rht_quo_replace() {
        let cases = vec![("It\u{2019}s a beautiful day!", "It's a beautiful day!")];

        for (input, expected) in cases {
            assert_eq!(rht_quo_replace(input.to_string()), expected.to_string());
        }
    }

    #[test]
    fn test_zwsp_remove() {
        let cases = vec![
            ("Hello\u{200b}world", "Helloworld"),
            (
                "\u{200b}Leading and trailing\u{200b}",
                "Leading and trailing",
            ),
            ("Zero\u{200b}width\u{200b}spaces", "Zerowidthspaces"),
            ("Regular spaces only", "Regular spaces only"),
            ("", ""),
        ];

        for (input, expected) in cases {
            assert_eq!(zwsp_remove(input.to_string()), expected.to_string());
        }
    }

    #[test]
    fn test_dot_remove() {
        let cases = vec![
            ("Hello world.", "Hello world"),
            ("Sun.rise", "Sunrise"),
            (".", ""),
        ];

        for (input, expected) in cases {
            assert_eq!(dot_remove(input.to_string()), expected.to_string());
        }
    }

    #[test]
    fn test_name_clean_valid() {
        let cases = vec![
            ("Dr. John Doe", "John Doe"),
            ("Jane A. Smith PhD", "Jane A. Smith"),
            ("John Q. Public, JD", "John Q. Public"),
            ("\"The Great\" Dr. John Doe", "John Doe"),
            ("Dr. Jane A. Doe Ph.D.", "Jane A. Doe"),
            ("Dr. John Doe, PhD", "John Doe"),
            ("Ed. D. Jane Smith", "Jane Smith"),
            ("J. D. John Public", "John Public"),
            ("Alice Doe MPH CIH", "Alice Doe"),
            ("Maximum (Max) Name", "Maximum Name"),
            ("A.C. Public", "A.C. Public"),
            ("John\u{a0}Quincy", "John Quincy"),
            ("O\u{2019}Connor", "O'Connor"),
        ];

        for (input, expected) in cases {
            assert_eq!(name_clean(input), expected);
        }
    }

    #[test]
    fn test_name_clean_invalid() {
        let cases = vec![
            ("John Doe", "John Doe"),
            ("Jane Smith", "Jane Smith"),
            ("Jane Doe", "Jane Doe"),
            ("John Doe, Esq.", "John Doe, Esq."),
            ("Mr. John Smith", "Mr. John Smith"),
        ];

        for (input, expected) in cases {
            assert_eq!(name_clean(input), expected);
        }
    }

    #[test]
    fn test_name_clean_split_valid() {
        let cases = vec![
            ("Dr. John Doe", ("John".to_string(), "Doe".to_string())),
            (
                "Jane A. Smith PhD",
                ("Jane".to_string(), "Smith".to_string()),
            ),
            (
                "John Q. Public, JD",
                ("John".to_string(), "Public".to_string()),
            ),
            (
                "\"The Great\" Dr. John Doe",
                ("John".to_string(), "Doe".to_string()),
            ),
            (
                "Dr. Jane A. Doe Ph.D.",
                ("Jane".to_string(), "Doe".to_string()),
            ),
            ("Dr. John Doe, PhD", ("John".to_string(), "Doe".to_string())),
            (
                "Ed. D. Jane Smith",
                ("Jane".to_string(), "Smith".to_string()),
            ),
            (
                "J. D. John Public",
                ("John".to_string(), "Public".to_string()),
            ),
            (
                "John Quincy Public",
                ("John".to_string(), "Quincy Public".to_string()),
            ),
        ];

        for (input, expected) in cases {
            assert_eq!(name_clean_split(input), expected);
        }
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
