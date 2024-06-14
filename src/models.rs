use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
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
#[derive(Debug, Default, Serialize, Deserialize, Clone)]
pub struct Person {
    pub name_fst: String,
    pub name_lst: String,
    pub title1: String,
    pub title2: String,
    pub url: String,
    pub url_known: Option<String>,
    pub adrs: Option<Vec<Address>>,
}
impl Person {
    pub fn clone_url_known(&self) -> Self {
        Self {
            name_fst: self.name_fst.clone(),
            name_lst: self.name_lst.clone(),
            url_known: self.url_known.clone(),
            ..Default::default()
        }
    }
    pub fn merge_url_known(&mut self, src: &Person) {
        self.url_known.clone_from(&src.url_known);
    }
}
pub fn clone_url_known(pers: &[Person]) -> Vec<Person> {
    pers.iter().map(|v| v.clone_url_known()).collect()
}
pub fn merge_url_known(srcs: &[Person], dsts: &mut [Person]) {
    for (dst, src) in dsts.iter_mut().zip(srcs.iter()) {
        dst.merge_url_known(src)
    }
}
impl fmt::Display for Person {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "{},{},{},{},{}",
            // .as_deref().unwrap_or("")
            self.name_fst,
            self.name_lst,
            self.title1,
            self.title2,
            self.url
        )
    }
}
impl PartialEq for Person {
    fn eq(&self, other: &Self) -> bool {
        self.name_lst == other.name_lst && self.name_fst == other.name_fst
    }
}
impl Eq for Person {}
impl PartialOrd for Person {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for Person {
    fn cmp(&self, other: &Self) -> Ordering {
        match self.name_lst.cmp(&other.name_lst) {
            Ordering::Equal => self.name_fst.cmp(&other.name_fst),
            other => other,
        }
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
