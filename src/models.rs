use crate::core::*;
use crate::prsr::*;
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
    Observer,
}
impl fmt::Display for Role {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            Role::Military => write!(f, "Military"),
            Role::Scientific => write!(f, "Scientific"),
            Role::Political => write!(f, "Political"),
            Role::Observer => write!(f, "Observer"),
        }
    }
}

/// A person.
#[derive(Debug, Default, Serialize, Deserialize, Clone)]
pub struct Person {
    pub name: String,
    pub title1: String,
    pub title2: String,
    pub url: String,
    pub adrs: Option<Vec<Address>>,
}
impl fmt::Display for Person {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "{},{},{},{}",
            self.name, self.title1, self.title2, self.url
        )
    }
}
impl PartialEq for Person {
    fn eq(&self, other: &Self) -> bool {
        self.name == other.name
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
        self.name.cmp(&other.name)
    }
}
impl Person {
    pub fn adr_len(&self) -> usize {
        self.adrs
            .as_ref() // Get a reference to the Option<Vec<Address>>
            .map(|adrs| adrs.len()) // Map the Option to the length of the vector if it exists
            .unwrap_or(0) // Return 0 if the Option is None
    }
}

/// A mailing address.
#[derive(Debug, Default, Serialize, Deserialize, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Address {
    pub address1: String,
    pub address2: Option<String>,
    pub city: String,
    pub state: String,
    pub zip5: u32,
    pub zip4: u16,
    pub delivery_point: Option<String>,
}
impl fmt::Display for Address {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "{},{},{},{},{},{},{}",
            self.address1,
            self.address2.as_deref().unwrap_or(""),
            self.city,
            self.state,
            self.zip5,
            self.zip4,
            self.delivery_point.as_deref().unwrap_or("")
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

/// A mail piece for the USPS.
#[derive(Debug, Default, Serialize, Deserialize, Clone)]
pub struct Mailpiece {
    pub name: String,
    pub title1: Option<String>,
    pub title2: Option<String>,
    pub address1: String,
    pub city: String,
    pub state: String,
    pub zip5: u32,
    pub zip4: u16,
    pub delivery_point: Option<String>,
    pub barcode: String,
    pub id: u32,
}
impl fmt::Display for Mailpiece {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "{},{},{},{},{},{},{},{},{},{}",
            self.name,
            self.title1.as_deref().unwrap_or(""),
            self.title2.as_deref().unwrap_or(""),
            self.address1,
            self.city,
            self.state,
            self.zip5,
            self.zip4,
            self.delivery_point.as_deref().unwrap_or(""),
            self.id,
        )
    }
}

#[derive(Debug, Default, Serialize, Deserialize, Clone)]
pub struct Letter {
    pub to: String,
    pub paragraphs: Vec<String>,
    pub from: String,
}
