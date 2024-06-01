use crate::models::Address;
use anyhow::{anyhow, Result};
use reqwest::StatusCode;
use std::env;
use usps_addresses_sdk::apis::configuration::Configuration as CfgAdr;
use usps_addresses_sdk::apis::resources_api::get_address;
use usps_oauth_sdk::apis::configuration::Configuration as CfgAuth;
use usps_oauth_sdk::apis::default_api::post_token;
use usps_oauth_sdk::models::InlineResponse2001::ProviderAccessTokenResponse;

const CLIENT_KEY: &str = "USPS_CLIENT_KEY";
const CLIENT_SECRET: &str = "USPS_CLIENT_SECRET";

#[derive(Default)]
pub struct UspsClient {
    client_key: Option<String>,
    client_secret: Option<String>,
    cfg_adr: Option<CfgAdr>,
}

impl UspsClient {
    pub fn new() -> UspsClient {
        UspsClient::default()
    }
    pub fn read_key_secret(&mut self) -> Result<()> {
        let client_key = env::var(CLIENT_KEY)?;
        let client_secret = env::var(CLIENT_SECRET)?;

        self.client_key = Some(client_key);
        self.client_secret = Some(client_secret);

        Ok(())
    }
    pub async fn fetch_token(&mut self) -> Result<()> {
        // Load the key and secret.
        if self.client_key.is_none() {
            self.read_key_secret()?;
        }

        // Fetch the access token.
        let cfg = CfgAuth::new();
        match post_token(
            &cfg,
            Some("client_credentials"),
            None,
            self.client_key.as_deref(),
            self.client_secret.as_deref(),
            None,
            None,
            None,
        )
        .await
        {
            Ok(inline_res) => {
                // eprintln!("ok:{inline_res:?}");
                if let ProviderAccessTokenResponse(res) = inline_res {
                    let mut cfg_adr = CfgAdr::new();
                    cfg_adr.oauth_access_token = Some(res.access_token);
                    self.cfg_adr = Some(cfg_adr);
                    Ok(())
                } else {
                    Err(anyhow!("unknown response"))
                }
            }
            Err(err) => Err(err.into()),
        }
    }
    pub async fn standardize_address(&mut self, adr: &mut Address) -> Result<()> {
        // eprintln!("adr:{adr:?}");
        // Fetch access token as needed.
        if self.cfg_adr.is_none() {
            // WARNING: TOKEN REFRESH SCENARIO NOT HANDLED.
            //  WHEN TOKEN EXPIRES AN UNRECOVERABLE ERROR.
            self.fetch_token().await?;
        }
        match get_address(
            self.cfg_adr.as_ref().unwrap(),
            adr.address1.as_str(),
            adr.state.as_str(),
            adr.address2.as_deref(),
            None,
            None,
            Some(adr.zip.as_str()),
            None,
        )
        .await
        {
            Ok(res) => {
                eprintln!("std:{res:?}");
                if let Some(adr_std) = res.address {
                    if let Some(address1) = adr_std.street_address {
                        adr.address1 = address1;
                    }
                    if let Some(address1) = adr_std.street_address_abbreviation {
                        adr.address1 = address1;
                    }
                    adr.address2 = adr_std.secondary_address;
                    if let Some(city) = adr_std.city {
                        adr.city = city;
                    }
                    if let Some(zip) = adr_std.zip_code {
                        adr.zip = zip;
                    }
                    if let Some(zip4) = adr_std.zip_plus4 {
                        adr.zip.push('-');
                        adr.zip.push_str(zip4.as_str());
                    }
                    eprintln!("  {adr}");
                    Ok(())
                } else {
                    Err(anyhow!("usps: response missing address"))
                }
            }
            Err(err) => {
                eprintln!("std:err:{adr:?}");
                eprintln!("std:err:{err:?}");
                // use usps_oauth_sdk::apis::Error::ResponseError;
                // use usps_oauth_sdk::apis::ResponseContent;
                if let usps_addresses_sdk::apis::Error::ResponseError(ref inr) = err {
                    if inr.status == StatusCode::BAD_REQUEST {
                        // TODO: try get address without zip code.
                    }
                }
                Err(err.into())
            }
        }
    }
}
