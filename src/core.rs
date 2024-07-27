use crate::models::*;
use anyhow::{anyhow, Result};
use chrono::NaiveDate;
use csv::Writer;
use reqwest::header::{HeaderMap, HeaderValue, USER_AGENT};
use reqwest::Client;
use scraper::{Html, Selector};
use serde::{Deserialize, Serialize};
use std::fs;
use std::fs::File;
use std::io::{self, Write};
use std::io::{BufReader, BufWriter};
use std::path::{Path, PathBuf};

lazy_static! {
    pub static ref CLI: Client = {
        // Create a header map and set the User-Agent header.
        // Set User-Agent to avoid url blocking.
        let mut headers = HeaderMap::new();
        headers.insert(USER_AGENT, HeaderValue::from_static("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/91.0.4472.124 Safari/537.36"));

        // Build the client with the custom headers
        Client::builder()
            .default_headers(headers)
            .build().unwrap()
    };
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

pub fn cache_dir() -> PathBuf {
    PathBuf::from(".cache")
}

/// Fetches HTML from a URL and caches the response body to a local file.
pub async fn fetch_html(url: &str) -> Result<String> {
    let mut pth = cache_dir();

    // Create the cache directory if it does not exist
    if !pth.exists() {
        fs::create_dir_all(&pth)?;
    }

    // Check if the cache file exists
    pth.push(url_to_filename(url));
    if pth.exists() {
        eprintln!("Loading cached HTML from {:?}...", &pth);
        let cached_body = fs::read_to_string(&pth)?;
        return Ok(cached_body);
    }

    eprintln!("Fetching {url:?}...");
    let res = CLI.get(url).send().await?;
    let bdy = res.text().await?;

    // Save the fetched body to the cache file
    let mut file = fs::File::create(&pth)?;
    file.write_all(bdy.as_bytes())?;

    Ok(bdy)
}

/// Fetches PDF from a URL and caches the response body to a local file.
pub async fn fetch_pdf(url: &str) -> Result<PathBuf> {
    let mut pth = cache_dir();

    // Create the cache directory if it does not exist
    if !pth.exists() {
        fs::create_dir_all(&pth)?;
    }

    // Check if the cache file exists
    pth.push(url_to_filename(url));
    if pth.exists() {
        return Ok(pth);
    }

    eprintln!("Fetching {url:?}...");
    let res = CLI.get(url).send().await?;
    let bdy = res.bytes().await?;

    // Save the fetched body to the cache file
    let mut file = fs::File::create(&pth)?;
    file.write_all(&bdy)?;

    Ok(pth)
}

/// Converts a URL to a safe filename by replacing non-alphanumeric characters.
fn url_to_filename(url: &str) -> String {
    // Skip https://
    url[8..]
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '_' })
        .collect()
}

/// Transforms a String to Option<String>.
/// Empty string is None.
pub fn string_to_opt(s: String) -> Option<String> {
    if s.is_empty() {
        None
    } else {
        Some(s)
    }
}

pub fn yr_qtr(date: NaiveDate) -> String {
    use chrono::Datelike;
    let year = date.year();
    let month = date.month();

    let quarter = match month {
        1..=3 => 1,
        4..=6 => 2,
        7..=9 => 3,
        10..=12 => 4,
        _ => unreachable!(),
    };

    format!("{}-Q{}", year, quarter)
}

/// Format an integer with commas.
pub fn numfmt(num: usize) -> String {
    let mut ret = String::new();
    for (i, c) in num.to_string().chars().rev().enumerate() {
        if i != 0 && i % 3 == 0 {
            ret.push(',');
        }
        ret.push(c);
    }
    ret.chars().rev().collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use reqwest::Client;
    use std::fs;
    use tokio::runtime::Runtime;

    #[test]
    fn test_numfmt() {
        assert_eq!(numfmt(0), "0");
        assert_eq!(numfmt(100), "100");
        assert_eq!(numfmt(1000), "1,000");
        assert_eq!(numfmt(10000), "10,000");
        assert_eq!(numfmt(100000), "100,000");
        assert_eq!(numfmt(1000000), "1,000,000");
        assert_eq!(numfmt(10000000), "10,000,000");
        assert_eq!(numfmt(100000000), "100,000,000");
        assert_eq!(numfmt(1000000000), "1,000,000,000");
    }
    
    #[test]
    fn test_valid_cases() {
        let test_cases = vec![
            (
                NaiveDate::from_ymd_opt(2024, 1, 1).expect("Invalid date"),
                "2024-Q1",
            ),
            (
                NaiveDate::from_ymd_opt(2024, 4, 1).expect("Invalid date"),
                "2024-Q2",
            ),
            (
                NaiveDate::from_ymd_opt(2024, 7, 1).expect("Invalid date"),
                "2024-Q3",
            ),
            (
                NaiveDate::from_ymd_opt(2024, 10, 1).expect("Invalid date"),
                "2024-Q4",
            ),
            (
                NaiveDate::from_ymd_opt(2024, 12, 31).expect("Invalid date"),
                "2024-Q4",
            ),
        ];

        for (input, expected) in test_cases {
            assert_eq!(yr_qtr(input), expected);
        }
    }

    #[test]
    fn test_string_to_opt_valid() {
        assert_eq!(
            string_to_opt("Hello".to_string()),
            Some("Hello".to_string())
        );
        assert_eq!(
            string_to_opt("world".to_string()),
            Some("world".to_string())
        );
        assert_eq!(string_to_opt("Rust".to_string()), Some("Rust".to_string()));
        assert_eq!(string_to_opt("".to_string()), None);
        assert_eq!(string_to_opt(String::new()), None);
    }

    #[test]
    fn test_fetch_html_with_caching() {
        let runtime = Runtime::new().unwrap();

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
