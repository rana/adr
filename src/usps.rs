use crate::models::Address;
use anyhow::{anyhow, Result};
use reqwest::Client;
use reqwest::StatusCode;
use serde::{Deserialize, Serialize};

pub async fn standardize_addresses(adrs: &mut Vec<Address>, cli: &Client) -> Result<()> {
    // Reverse indexes to allow for removal of invalid addresses.
    for idx in (0..adrs.len()).rev() {
        match standardize_address(&mut adrs[idx], cli).await {
            Ok(_) => {
                // The USPS prefers that secondary address designators such as "APT" (Apartment) or "STE" (Suite) appear on the same line as the street address when there is enough space. However, it is also acceptable for these designators to appear on a separate line if needed, typically as Address Line 2.

                // // Edit edge cases:
                // // 2743 PERIMETR PKWY BLDG 200 STE 105,STE 105,AUGUSTA,GA
                // if let Some(idx_fnd) = adrs[idx].address1.find(" BLDG ") {
                //     let mut address2 = adrs[idx].address2.clone().unwrap_or_default();
                //     address2.push_str(&adrs[idx].address1[idx_fnd..]);
                //     adrs[idx].address2 = Some(address2.trim().into());
                //     adrs[idx].address1.truncate(idx_fnd);
                // }
                // // 685 CARNEGIE DR STE 100,,SAN BERNARDINO,CA
                // else if let Some(idx_fnd) = adrs[idx].address1.rfind(" STE ") {
                //     let mut address2 = adrs[idx].address2.clone().unwrap_or_default();
                //     address2.push_str(&adrs[idx].address1[idx_fnd..]);
                //     adrs[idx].address2 = Some(address2.trim().into());
                //     adrs[idx].address1.truncate(idx_fnd);
                // }
                // // 1070 MAIN ST UNIT 300,,PAWTUCKET,RI
                // else if let Some(idx_fnd) = adrs[idx].address1.rfind(" UNIT ") {
                //     let mut address2 = adrs[idx].address2.clone().unwrap_or_default();
                //     address2.push_str(&adrs[idx].address1[idx_fnd..]);
                //     adrs[idx].address2 = Some(address2.trim().into());
                //     adrs[idx].address1.truncate(idx_fnd);
                // }
                // // 220 E ROSSER AVENUE RM 228,US FEDERAL BUILDING,BISMARCK,,ND,58501
                // else if let Some(idx_fnd) = adrs[idx].address1.rfind(" RM ") {
                //     let mut address2 = adrs[idx].address2.clone().unwrap_or_default();
                //     address2.push_str(&adrs[idx].address1[idx_fnd..]);
                //     adrs[idx].address2 = Some(address2.trim().into());
                //     adrs[idx].address1.truncate(idx_fnd);
                // }
                // // 1 GOVERNMENT CTR OFC 237B,,FALL RIVER,MA,02722-7700
                // else if let Some(idx_fnd) = adrs[idx].address1.rfind(" OFC ") {
                //     let mut address2 = adrs[idx].address2.clone().unwrap_or_default();
                //     address2.push_str(&adrs[idx].address1[idx_fnd..]);
                //     adrs[idx].address2 = Some(address2.trim().into());
                //     adrs[idx].address1.truncate(idx_fnd);
                // }
            }
            Err(err) => {
                eprintln!("standardize_addresses: err: {}", err);
                // Mitigate failed address standardization.
                // Some non-addresses may be removed here.
                // INVALID ADDRESSES
                // https://amodei.house.gov/
                // Address { first_name: "Mark", last_name: "Amodei", address1: "89511", address2: Some("Elko Contact"), city: "Elko", state: "NV", zip: "89801" }
                if adrs[idx].zip == "89801" {
                    eprintln!("removed invalid address: {:?}", adrs[idx]);
                    adrs.remove(idx);
                } else {
                    eprintln!("Attempting to standardise address without zip.");
                    adrs[idx].zip = "".into();
                    eprintln!("  {}", adrs[idx]);
                    standardize_address(&mut adrs[idx], cli).await?;
                    // return Err(err);
                }
            }
        }
    }

    Ok(())
}

pub async fn standardize_address(adr: &mut Address, cli: &Client) -> Result<()> {
    let client = Client::new();

    let mut prms: Vec<(&str, String)> = Vec::with_capacity(5);
    if !adr.address1.is_empty() {
        prms.push(("address1", adr.address1.clone()));
    }
    if adr.address2.is_some() {
        let address2 = adr.address2.clone().unwrap();
        prms.push(("address2", address2));
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

    if response_json.resultStatus == "SUCCESS" {
        if !response_json.addressList.is_empty() {
            if let Some(new_adr) = response_json
                .addressList
                .into_iter()
                .find(|v| !v.addressLine1.contains("Range"))
            {
                from(adr, new_adr);
                Ok(())
            } else {
                Err(anyhow!(
                    "Over filtered response. No address found in the USPS response."
                ))
            }
        } else {
            Err(anyhow!("No address found in the USPS response."))
        }
    } else {
        Err(anyhow!("Failed to standardize address."))
    }
}

#[derive(Debug, Deserialize)]
pub struct USPSResponse {
    resultStatus: String,
    addressList: Vec<USPSAddress>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct USPSAddress {
    companyName: Option<String>,
    addressLine1: String,
    addressLine2: Option<String>,
    city: String,
    state: String,
    zip5: String,
    zip4: String,
}

fn from(adr: &mut Address, usps: USPSAddress) {
    adr.address1 = usps.addressLine1;
    adr.address2 = usps.addressLine2;
    adr.city = usps.city;
    adr.state = usps.state;
    adr.zip = format!("{}-{}", usps.zip5, usps.zip4);
}
