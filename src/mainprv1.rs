use openai_api_rs::v1::api::Client;
use openai_api_rs::v1::chat_completion::{self, ChatCompletionRequest};
use openai_api_rs::v1::common::GPT4_O_2024_05_13;
use std::env;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    match env::var("OPENAI_TOKEN") {
        Ok(api_key) => {
            eprintln!("read token: {}", api_key);
            let client = Client::new(api_key);

            let url = "https://wilson.house.gov/";

            let req = ChatCompletionRequest::new(
                GPT4_O_2024_05_13.to_string(),
                vec![chat_completion::ChatCompletionMessage {
                    role: chat_completion::MessageRole::user,
                    content: chat_completion::Content::Text(format!("Search site {url}. List mailing addresses.")),
                    name: None,
                }],
            );

            match client.chat_completion(req) {
                Ok(res) => {
                    println!("Content: {:?}", res.choices[0].message.content);
                    println!("Response Headers: {:?}", res.headers);
                }
                Err(err) => eprintln!("res err: {:?}", err),
            }
        }
        Err(err) => {
            eprintln!("read token: err: {}", err);
        }
    }

    Ok(())
}
