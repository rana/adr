use reqwest::{Client, StatusCode};
use scraper::{Html, Selector};

#[tokio::main]
async fn main() -> Result<(), reqwest::Error> {
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
