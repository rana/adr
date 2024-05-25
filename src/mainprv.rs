use reqwest::{Client, StatusCode};
use scraper::{Html, Selector};
use csv::Writer;
use std::error::Error;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    // Initialize CSV writer
    let mut wtr = Writer::from_path("addresses.csv")?;

    // Scrape House of Representatives
    scrape_and_process("https://www.house.gov/representatives", &mut wtr).await?;

    // // Scrape Senate
    // scrape_and_process("https://www.senate.gov/senators", &mut wtr).await?;

    // Continue with other required pages...

    Ok(())
}

async fn scrape_and_process(url: &str, wtr: &mut Writer<std::fs::File>) -> Result<(), Box<dyn Error>> {
    eprintln!("Fetching URL: {}", url);
    let resp = reqwest::get(url).await?.text().await?;

    eprintln!("Parsing HTML");
    let document = Html::parse_document(&resp);
    let selector = Selector::parse(".target-selector").unwrap(); // Adjust selector based on actual HTML

    for element in document.select(&selector) {
        let name = element.select(&Selector::parse(".name-selector").unwrap()).next().unwrap().text().collect::<String>();
        let address = element.select(&Selector::parse(".address-selector").unwrap()).next().unwrap().text().collect::<String>();

        // Normalize the address
        let normalized_address = normalize_address(&address).await?;

        // Write to CSV
        wtr.write_record(&[name, normalized_address])?;
        eprintln!("Processed: {}, {}", name, normalized_address);
    }

    Ok(())
}

async fn normalize_address(address: &str) -> Result<String, Box<dyn Error>> {
    // Here you would call the USPS API or another service to normalize the address
    Ok(address.to_string()) // Placeholder for the actual normalization process
}

// #[tokio::main]
async fn main_prv() -> Result<(), reqwest::Error> {
    let url = "https://www.house.gov/representatives".to_string();
    eprintln!("Fetching {url:?}...");

    // reqwest::get() is a convenience function.
    //
    // In most cases, you should create/build a reqwest::Client and reuse
    // it for all requests.
    // let res = reqwest::get(url).await?;
    // eprintln!("Response: {:?} {}", res.version(), res.status());
    // eprintln!("Headers: {:#?}\n", res.headers());
    // let body = res.text().await?;
    // println!("{body}");
    let cli = Client::new();
    match cli.get(url).send().await {
        Ok(res) => match res.status() {
            StatusCode::OK => {
                match res.text().await {
                    Ok(txt) => {
                        // eprintln!("Success! {:?}", txt);
                        let doc = Html::parse_document(&txt);
                        let sel = Selector::parse("#by-state").unwrap();

                        for nod in doc.select(&sel) {
                            eprintln!("{:?}", nod.value());

                            let sel = Selector::parse(".view-content").unwrap();
                            for nod in nod.select(&sel) {
                                eprintln!("{:?}", nod.value());

                                let sel = Selector::parse("table").unwrap();
                                for nod in nod.select(&sel) {
                                    eprintln!("{:?}", nod.value());

                                    let sel = Selector::parse("tbody").unwrap();
                                    for nod in nod.select(&sel) {
                                        eprintln!("{:?}", nod.value());

                                        let sel = Selector::parse("a").unwrap();
                                        for nod in nod.select(&sel) {
                                            eprintln!("{:?}, {:?}", nod.value(), nod.inner_html());
                                        }
                                    }
                                }
                            }
                        }
                    }
                    Err(err) => eprintln!("res: parse: err: {:?}", err),
                };
            }
            other => eprintln!("res: status: {:?}", other),
        },
        Err(err) => eprintln!("res err: {:?}", err),
    }

    Ok(())
}
