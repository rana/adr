use crate::core::*;
use crate::models::*;
use anyhow::{anyhow, Result};
use reqwest::Client;
use reqwest::StatusCode;
use serde::{Deserialize, Serialize};
use StdAdr::*;

pub async fn standardize_addresses(mut adrs: Vec<Address>) -> Result<Vec<Address>> {
    // The USPS prefers that secondary address designators such as "APT" (Apartment) or "STE" (Suite) appear on the same line as the street address when there is enough space. However, it is also acceptable for these designators to appear on a separate line if needed, typically as Address Line 2.
    eprintln!("{}", AddressList(adrs.clone()));

    for adr in adrs.iter_mut() {
        eprintln!("Attempting to standardize by combining address lines.");
        match standardize_address(adr, AsIs, false).await {
            Ok(_) => {}
            Err(err) => {
                eprintln!("standardize_addresses: err1: {}", err);

                eprintln!("Attempting to standardize without combining address lines.");
                match standardize_address(adr, CombineAdr1Adr2, false).await {
                    Ok(_) => {}
                    Err(err) => {
                        eprintln!("standardize_addresses: err2: {}", err);

                        eprintln!("Attempting to standardize by swapping address lines.");
                        match standardize_address(adr, SwapAdr1Adr2, false).await {
                            Ok(_) => {}
                            Err(err) => {
                                eprintln!("standardize_addresses: err3: {}", err);

                                // Mitigate failed address standardization.
                                eprintln!("Attempting to standardize address without zip.");
                                adr.zip5 = 0;
                                eprintln!("  {}", adr);
                                standardize_address(adr, AsIs, true).await?;
                            }
                        }
                    }
                }
            }
        }
    }

    // Deduplicate extracted addresses.
    adrs.sort_unstable();
    adrs.dedup_by(|a, b| a == b);

    eprintln!("{}", AddressList(adrs.clone()));

    Ok(adrs)
}

#[derive(PartialEq)]
pub enum StdAdr {
    AsIs,
    CombineAdr1Adr2,
    SwapAdr1Adr2,
}
pub async fn standardize_address(
    adr: &mut Address,
    approach: StdAdr,
    drop_zip: bool,
) -> Result<()> {
    let mut prms: Vec<(&str, String)> = Vec::with_capacity(5);
    match approach {
        AsIs => {
            if !adr.address1.is_empty() {
                prms.push(("address1", adr.address1.clone()));
            }
            if adr.address2.is_some() {
                let address2 = adr.address2.clone().unwrap();
                prms.push(("address2", address2));
            }
        }
        CombineAdr1Adr2 => {
            let mut address1 = adr.address1.clone();
            if let Some(address2) = adr.address2.clone() {
                address1.push(' ');
                address1.push_str(&address2);
            }
            prms.push(("address1", address1));
        }
        SwapAdr1Adr2 => {
            if adr.address2.is_some() {
                let address2 = adr.address2.clone().unwrap();
                prms.push(("address1", address2));
            } else {
                return Err(anyhow!("No address2 to swap to address1."));
            }
        }
    }

    if !adr.city.is_empty() {
        prms.push(("city", adr.city.clone()));
    }
    if !adr.state.is_empty() {
        prms.push(("state", adr.state.clone()));
    }
    if !drop_zip && adr.zip5 != 0 {
        prms.push(("zip", format!("{:05}", adr.zip5)));
    }

    let response = CLI
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

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct USPSAddress {
    company_name: Option<String>,
    address_line1: String,
    address_line2: Option<String>,
    city: String,
    state: String,
    zip5: String,
    zip4: String,
    delivery_point: Option<String>,
}

fn from(adr: &mut Address, usps: USPSAddress) {
    adr.address1 = usps.address_line1;
    adr.address2 = usps.address_line2;
    adr.city = usps.city;
    adr.state = usps.state;
    if usps.zip4.is_empty() {
        adr.zip5 = usps.zip5.parse().unwrap();
    } else {
        adr.zip5 = usps.zip5.parse().unwrap();
        adr.zip4 = usps.zip4.parse().unwrap();
    }
    adr.delivery_point = usps.delivery_point;
}

/// Encodes mailing information to characters
/// `F`,`A`,`D`,`T`
/// for use with a barcode font.
pub async fn encode_barcode(
    barcode_id: &str,
    service_id: &str, // STID
    mailer_id: &str,
    serial_id: &str,
    routing_code: &str,
) -> Result<String> {
    // Validate input.
    if barcode_id.len() != 2
        || !barcode_id.chars().all(|c| c.is_ascii_digit())
        || barcode_id.chars().nth(1).unwrap() > '4'
    {
        return Err(anyhow!("Invalid barcode_id"));
    }
    if service_id.len() != 3 || !service_id.chars().all(|c| c.is_ascii_digit()) {
        return Err(anyhow!("Invalid service_id"));
    }
    if mailer_id.len() != 9 || !mailer_id.chars().all(|c| c.is_ascii_digit()) {
        return Err(anyhow!("Invalid mailer_id"));
    }
    if serial_id.len() != 6 || !serial_id.chars().all(|c| c.is_ascii_digit()) {
        return Err(anyhow!("Invalid serial_id"));
    }
    if !(routing_code.is_empty()
        || routing_code.len() == 5
        || routing_code.len() == 9
        || routing_code.len() == 11)
        || !routing_code.chars().all(|c| c.is_ascii_digit())
    {
        return Err(anyhow!("Invalid zip_code"));
    }

    // Encode information.
    let qry = format!(
        "{}{}{}{}{}",
        barcode_id, service_id, mailer_id, serial_id, routing_code
    );
    // eprintln!("qry:{qry}");
    let url = format!(
        "https://postalpro.usps.com/ppro-tools-api/imb/encode?imb={}",
        qry
    );
    eprintln!("url:{url}");

    let res = CLI.get(&url).send().await?.json::<ImbResponse>().await?;

    if res.code != "00" {
        return Err(anyhow!("Error from API: {}", res.code));
    }

    // Return the encoding.
    Ok(res.imb)
}
#[derive(Deserialize)]
struct ImbResponse {
    code: String,
    imb: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_valid_barcode() {
        let barcode_id = "50";
        let service_id = "301";
        let mailer_id = "899999999";
        let serial_id = "981000";
        let zip_code = "12345";

        let result = encode_barcode(barcode_id, service_id, mailer_id, serial_id, zip_code).await;
        assert!(result.is_ok());
        assert!(!result.unwrap().is_empty());
    }

    #[tokio::test]
    async fn test_invalid_barcode_id() {
        let barcode_id = "5a"; // Invalid
        let service_id = "301";
        let mailer_id = "899999999";
        let serial_id = "981000";
        let zip_code = "01926";

        let result = encode_barcode(barcode_id, service_id, mailer_id, serial_id, zip_code).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_invalid_service_id() {
        let barcode_id = "50";
        let service_id = "30a"; // Invalid
        let mailer_id = "899999999";
        let serial_id = "981000";
        let zip_code = "01926";

        let result = encode_barcode(barcode_id, service_id, mailer_id, serial_id, zip_code).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_invalid_mailer_id() {
        let barcode_id = "50";
        let service_id = "301";
        let mailer_id = "89999999a"; // Invalid
        let serial_id = "981000";
        let zip_code = "01926";

        let result = encode_barcode(barcode_id, service_id, mailer_id, serial_id, zip_code).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_invalid_serial_id() {
        let barcode_id = "50";
        let service_id = "301";
        let mailer_id = "899999999";
        let serial_id = "98100a"; // Invalid
        let zip_code = "01926";

        let result = encode_barcode(barcode_id, service_id, mailer_id, serial_id, zip_code).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_invalid_zip_code() {
        let barcode_id = "50";
        let service_id = "301";
        let mailer_id = "899999999";
        let serial_id = "981000";
        let zip_code = "0192a"; // Invalid

        let result = encode_barcode(barcode_id, service_id, mailer_id, serial_id, zip_code).await;
        assert!(result.is_err());
    }
}
