use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use std::default;
use std::fmt;

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum Role {
    Military,
    Scientific,
    Political,
}
impl fmt::Display for Role {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            Role::Military => write!(f, "Military"),
            Role::Scientific => write!(f, "Scientific"),
            Role::Political => write!(f, "Political"),
        }
    }
}

/// A person.
#[derive(Debug, Default, Serialize, Deserialize, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Person {
    pub name_fst: String,
    pub name_lst: String,
    pub title1: String,
    pub title2: String,
    pub url: String,
    pub adrs: Option<Vec<Address>>,
}
impl fmt::Display for Person {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "{},{},{},{},{}",
            self.name_fst, self.name_lst, self.title1, self.title2, self.url
        )
    }
}

/// A mailing address.
#[derive(Debug, Default, Serialize, Deserialize, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Address {
    pub address1: String,
    pub address2: Option<String>,
    pub city: String,
    pub state: String,
    pub zip: String,
}
impl Address {
    pub fn is_valid(&self) -> bool {
        self.address1.len() <= 40
            && self.address2.as_ref().map_or(true, |s| s.len() <= 40)
            && self.city.len() <= 40
            && self.state.len() <= 2 // USPS state abbreviations are always 2 characters
            && self.zip.len() <= 10 // ZIP code can be 5 or 9 digits (with hyphen)
    }
}
impl fmt::Display for Address {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "{},{},{},{},{}",
            self.address1,
            self.address2.as_deref().unwrap_or(""),
            self.city,
            self.state,
            self.zip
        )
    }
}

// AddressList for pretty printing.
pub struct AddressList(pub Vec<Address>);
impl fmt::Display for AddressList {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        for (i, address) in self.0.iter().enumerate() {
            if i != 0 {
                writeln!(f)?;
            }
            write!(f, "  {}", address)?;
        }
        Ok(())
    }
}
