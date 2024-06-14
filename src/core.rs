use crate::models::*;
use anyhow::{anyhow, Result};
use csv::Writer;
use reqwest::Client;
use scraper::{Html, Selector};
use serde::{Deserialize, Serialize};
use std::fs;
use std::fs::File;
use std::io::{self, Write};
use std::io::{BufReader, BufWriter};
use std::path::{Path, PathBuf};

lazy_static! {
    pub static ref CLI: Client = Client::new();
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

/// Fetches HTML from a URL and caches the response body to a local file.
pub async fn fetch_html(url: &str) -> Result<String> {
    let cache_dir = Path::new(".cache");
    let cache_file = cache_dir.join(url_to_filename(url));

    // Create the cache directory if it does not exist
    if !cache_dir.exists() {
        fs::create_dir_all(cache_dir)?;
    }

    // Check if the cache file exists
    if cache_file.exists() {
        eprintln!("Loading cached HTML from {:?}...", cache_file);
        let cached_body = fs::read_to_string(&cache_file)?;
        return Ok(cached_body);
    }

    eprintln!("Fetching {url:?}...");
    let res = CLI.get(url).send().await?;
    let bdy = res.text().await?;

    // Save the fetched body to the cache file
    let mut file = fs::File::create(&cache_file)?;
    file.write_all(bdy.as_bytes())?;

    Ok(bdy)
}

/// Converts a URL to a safe filename by replacing non-alphanumeric characters.
fn url_to_filename(url: &str) -> String {
    // Skip https://
    url[8..]
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '_' })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use reqwest::Client;
    use std::fs;
    use tokio::runtime::Runtime;

    #[test]
    fn test_fetch_html_with_caching() {
        let runtime = Runtime::new().unwrap();
        let cli = Client::new();

        // Replace with a test URL
        let test_url = "https://www.google.com";

        // First call should fetch and cache the content
        let result = runtime.block_on(fetch_html(test_url));
        assert!(result.is_ok());
        let body = result.unwrap();
        assert!(!body.is_empty());

        // Second call should load from cache
        let result = runtime.block_on(fetch_html(test_url));
        assert!(result.is_ok());
        let cached_body = result.unwrap();
        assert_eq!(body, cached_body);

        // Clean up cache file
        let cache_file = Path::new("cache").join(url_to_filename(test_url));
        fs::remove_file(cache_file).unwrap();

        // Clean up cache directory if empty
        if fs::read_dir("cache").unwrap().next().is_none() {
            fs::remove_dir("cache").unwrap();
        }
    }
}
