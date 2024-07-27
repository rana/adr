use crate::core::*;
use crate::models::*;
use crate::prsr::*;
use crate::usps::*;
use anyhow::{anyhow, Result};
use reqwest::Client;
use scraper::{Html, Selector};
use serde::{Deserialize, Serialize};
use std::fs::File;
use std::io::{BufReader, BufWriter};
use std::ops::Add;
use std::path::Path;

const FLE_PTH: &str = "observer.json";

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Observer {
    pub name: String,
    pub role: Role,
    pub persons: Vec<Person>,
}
impl Observer {
    pub fn new() -> Self {
        Self {
            name: "Non-officials".into(),
            role: Role::Observer,
            persons: Vec::new(),
        }
    }
    pub async fn load() -> Result<Observer> {
        // Read file from disk.
        read_from_file::<Observer>(FLE_PTH)
    }
}
