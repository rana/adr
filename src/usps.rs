use crate::models::Address;
use anyhow::{anyhow, Result};
use reqwest::Client;
use reqwest::StatusCode;
use serde::{Deserialize, Serialize};

pub async fn standardize_addresses(adrs: &mut [Address], cli: &Client) -> Result<()> {
    // The USPS prefers that secondary address designators such as "APT" (Apartment) or "STE" (Suite) appear on the same line as the street address when there is enough space. However, it is also acceptable for these designators to appear on a separate line if needed, typically as Address Line 2.

    for adr in adrs.iter_mut() {
        match standardize_address(adr, true, cli).await {
            Ok(_) => {}
            Err(err) => {
                eprintln!("standardize_addresses: err1: {}", err);

                eprintln!("Attempting to standardize without combining address lines.");
                match standardize_address(adr, false, cli).await {
                    Ok(_) => {}
                    Err(err) => {
                        eprintln!("standardize_addresses: err2: {}", err);

                        // Mitigate failed address standardization.
                        eprintln!("Attempting to standardize address without zip.");
                        adr.zip = "".into();
                        eprintln!("  {}", adr);
                        standardize_address(adr, true, cli).await?;
                    }
                }
            }
        }
    }

    Ok(())
}

pub async fn standardize_address(
    adr: &mut Address,
    combine_address2: bool,
    cli: &Client,
) -> Result<()> {
    let client = Client::new();
    let mut prms: Vec<(&str, String)> = Vec::with_capacity(5);

    if combine_address2 {
        let mut address1 = adr.address1.clone();
        if let Some(address2) = adr.address2.clone() {
            if address2 != "SUPERSTITION PLAZA" {
                address1.push(' ');
                address1.push_str(&address2);
            }
        }
        prms.push(("address1", address1));
    } else {
        if !adr.address1.is_empty() {
            prms.push(("address1", adr.address1.clone()));
        }
        if adr.address2.is_some() {
            let address2 = adr.address2.clone().unwrap();
            prms.push(("address2", address2));
        }
    }

    if !adr.city.is_empty() {
        prms.push(("city", adr.city.clone()));
    }
    if !adr.state.is_empty() {
        prms.push(("state", adr.state.clone()));
    }
    if !adr.zip.is_empty() {
        prms.push(("zip", adr.zip.clone()));
    }

    let response = cli
        .post("https://tools.usps.com/tools/app/ziplookup/zipByAddress")
        .form(&prms)
        .send()
        .await?;
    let response_text = response.text().await?;
    eprintln!("{}", response_text);
    let response_json: USPSResponse = serde_json::from_str(&response_text)?;

    if response_json.result_status == "SUCCESS" {
        if !response_json.address_list.is_empty() {
            let usps_adrs: Vec<USPSAddress> = response_json
                .address_list
                .into_iter()
                .filter(|v| !v.address_line1.contains("Range"))
                .collect();

            match usps_adrs.len() {
                1 => {
                    from(adr, usps_adrs[0].clone());
                    Ok(())
                }
                n if n > 1 => {
                    if let Some(new_adr) = usps_adrs.iter().find(|v| v.address_line2.is_none()) {
                        from(adr, new_adr.clone());
                    } else {
                        from(adr, usps_adrs[0].clone());
                    }
                    Ok(())
                }
                _ => Err(anyhow!(
                    "Over filtered response. No address found in the USPS response."
                )),
            }
        } else {
            Err(anyhow!("No address found in the USPS response."))
        }
    } else {
        Err(anyhow!("Failed to standardize address."))
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct USPSResponse {
    result_status: String,
    address_list: Vec<USPSAddress>,
}

#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct USPSAddress {
    company_name: Option<String>,
    address_line1: String,
    address_line2: Option<String>,
    city: String,
    state: String,
    zip5: String,
    zip4: String,
}

fn from(adr: &mut Address, usps: USPSAddress) {
    adr.address1 = usps.address_line1;
    adr.address2 = usps.address_line2;
    adr.city = usps.city;
    adr.state = usps.state;
    if usps.zip4.is_empty() {
        adr.zip = usps.zip5;
    } else {
        adr.zip = format!("{}-{}", usps.zip5, usps.zip4);
    }
}
