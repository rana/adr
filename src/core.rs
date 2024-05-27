use crate::models::*;

use anyhow::{anyhow, Result};
use csv::Writer;
use reqwest::Client;
use scraper::{Html, Selector};
use serde::{Deserialize, Serialize};
use std::fs::File;
use std::io::{BufReader, BufWriter};
use std::path::Path;

/// Fetches HTML from a URL.
pub async fn fetch_html(url: &str, cli: &Client) -> Result<String> {
    eprintln!("Fetching {url:?}...");
    let res = cli.get(url).send().await?;
    let bdy = res.text().await?;
    Ok(bdy)
}

/// Serializes a JSON struct to a file.
pub fn write_to_file<T: Serialize>(data: &T, file_path: &str) -> Result<()> {
    eprintln!("Writing file: {}", file_path);
    let file = File::create(file_path)?;
    let writer = BufWriter::new(file);
    serde_json::to_writer_pretty(writer, &data)?;
    Ok(())
}

/// Deserializes a JSON struct from a file.
pub fn read_from_file<T: for<'de> Deserialize<'de>>(file_path: &str) -> Result<T> {
    eprintln!("Reading file: {}", file_path);
    let file = File::open(file_path)?;
    let reader = BufReader::new(file);
    let data = serde_json::from_reader(reader)?;
    Ok(data)
}
