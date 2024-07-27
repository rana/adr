use crate::core::*;
use crate::envelope::*;
use crate::models::*;
use crate::postage_statement::*;
use crate::prsr::*;
use crate::usps::*;
use anyhow::{anyhow, Result};
use chrono::Local;
use chrono::NaiveDate;
use itertools::*;
use pdf_doc::*;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt;
use std::fs;
use std::fs::File;
use std::io::{BufReader, BufWriter, Cursor};
use std::path::Path;
use std::path::PathBuf;
use TraySize::*;

const FLE_PTH: &str = "mailing.json";
const FLE_PTH_CFG: &str = "mailing_cfg.json";
const FLE_PTH_LTR: &str = "letter-template.json";

const PRC_FIVE_DIG: f64 = 0.173; // PS Form 3602-N
const PRC_MIXED_AADC: f64 = 0.208; // PS Form 3602-N

lazy_static! {
    /// A mailing configuration.
    pub static ref CFG: MailingCfg = read_from_file::<MailingCfg>(FLE_PTH_CFG).unwrap();
}

// TODO: ADD "Return Service Requested" TO ENVELOPE.

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Mailing {
    pub name: String,
    /// The date that addresses were validated with the USPS.
    ///
    /// Used on postage statement form ps3602n.
    pub adr_validation_date: NaiveDate,
    pub trays: Vec<MailTray>,
    pub mailpiece_cnt: u16,
    pub tray_1ft_cnt: u8,
    pub tray_2ft_cnt: u8,
    pub five_dig_cnt: u16,
    pub mixed_aadc_cnt: u16,
    pub postage_subtotal_five_dig: f64,
    pub postage_subtotal_mixed_aadc: f64,
    pub part_a_subtotal: f64,
}

impl Mailing {
    pub fn new() -> Self {
        Self {
            name: yr_qtr(Local::now().date_naive()).to_string(),
            adr_validation_date: Local::now().date_naive(),
            trays: Vec::new(),
            mailpiece_cnt: 0,
            tray_1ft_cnt: 0,
            tray_2ft_cnt: 0,
            five_dig_cnt: 0,
            mixed_aadc_cnt: 0,
            postage_subtotal_five_dig: 0.0,
            postage_subtotal_mixed_aadc: 0.0,
            part_a_subtotal: 0.0,
        }
    }

    pub async fn load(pers: &mut [Person]) -> Result<Mailing> {
        // Read file from disk.
        let mut mailing = match read_from_file::<Mailing>(FLE_PTH) {
            Ok(mailing_from_disk) => mailing_from_disk,
            Err(_) => {
                let mut mailing = Mailing::new();

                // Create mailpieces for each person.
                let adr_cnt = pers.iter().map(|p| p.adr_len()).sum::<usize>();
                let mut mailpieces = Vec::with_capacity(adr_cnt);
                for per in pers.iter() {
                    if let Some(adrs) = &per.adrs {
                        for adr in adrs {
                            // See guidelines.
                            // https://about.usps.com/publications/pub28/28c2_007.htm
                            let mp = Mailpiece {
                                name: per.name.clone(),
                                title1: string_to_opt(per.title1.clone()),
                                title2: string_to_opt(per.title2.clone()),
                                address1: adr.address1.clone(),
                                city: adr.city.clone(),
                                state: adr.state.clone(),
                                zip5: adr.zip5,
                                zip4: adr.zip4,
                                delivery_point: adr.delivery_point.clone(),
                                ..Default::default()
                            };
                            mailpieces.push(mp);
                        }
                    } else {
                        return Err(anyhow!("missing address for {}", per));
                    }
                }

                // Set mailpiece count.
                mailing.mailpiece_cnt = mailpieces.len() as u16;

                // Sort by zip code for id generation.
                mailpieces.sort_unstable_by_key(|o| format!("{:05}{:04}", o.zip5, o.zip4));

                // Calculate current id based on the previous mailing
                // and current mailing. Each envelope gets a unique id.
                // Id is used in the barcode.
                let mut base_id = CFG.last_mailpiece_id + 1;
                for (idx, mp) in mailpieces.iter_mut().enumerate() {
                    mp.id = base_id + idx as u32;
                }

                // Pre-sort for USPS discount.
                mailing.trays = presort_mailpieces(mailpieces);
                eprintln!("{} trays", mailing.trays.len());

                // Determine tray counts.
                mailing.tray_1ft_cnt = mailing
                    .trays
                    .iter()
                    .filter(|o| o.size == TraySize::OneFoot)
                    .count() as u8;
                mailing.tray_2ft_cnt = mailing
                    .trays
                    .iter()
                    .filter(|o| o.size == TraySize::TwoFoot)
                    .count() as u8;

                // Determine price categories.
                mailing.five_dig_cnt = mailing
                    .trays
                    .iter()
                    .filter(|o| o.barcode_id == BarcodeId::FiveDigit)
                    .map(|o| o.mailpieces.len())
                    .sum::<usize>() as u16;
                mailing.mixed_aadc_cnt = mailing
                    .trays
                    .iter()
                    .filter(|o| o.barcode_id == BarcodeId::MixedAadc)
                    .map(|o| o.mailpieces.len())
                    .sum::<usize>() as u16;

                // Calculate prices.
                mailing.postage_subtotal_five_dig = mailing.five_dig_cnt as f64 * PRC_FIVE_DIG;
                mailing.postage_subtotal_mixed_aadc = mailing.mixed_aadc_cnt as f64 * PRC_MIXED_AADC;
                mailing.part_a_subtotal = mailing.postage_subtotal_five_dig + mailing.postage_subtotal_mixed_aadc;

                // Write file to disk.
                write_to_file(&mailing, FLE_PTH)?;

                mailing
            }
        };

        // Create the directory and any necessary parent directories
        let mut pth = PathBuf::from("mailings");
        pth.push(&mailing.name);
        if pth.exists() {
            // Delete any previous directory.
            fs::remove_dir_all(&pth)?;
        }
        fs::create_dir_all(&pth)?;

        // // Find longest title1.
        // pers.sort_unstable_by_key(|k| k.title1.len());
        // eprintln!("title1:{}", pers[pers.len() - 1].title1);

        // // Find longest address1.
        // mailpieces.sort_unstable_by_key(|k| k.address1.len());
        // eprintln!("address1:{}", mailpieces[mailpieces.len() - 1].address1);

        let mps_len = mailing
            .trays
            .iter()
            .map(|o| o.mailpieces.len())
            .sum::<usize>() as f64;

        // Add barcodes to mailpieces.
        // Mail tray barcode_id is used in the barcode.
        let mut may_write = false;
        let mut cur_cnt: usize = 0;
        for mail_tray in mailing.trays.iter_mut() {
            if mail_tray.add_barcodes(cur_cnt, mps_len).await? {
                may_write = true;
            }
            cur_cnt += mail_tray.mailpieces.len();
        }
        if may_write {
            // Save intermediate.
            // Write file to disk.
            write_to_file(&mailing, FLE_PTH)?;
        }

        // Create envelopes and letters.
        let mut cur_cnt: usize = 0;
        for mail_tray in mailing.trays.iter() {
            mail_tray.create_envelopes_letters(cur_cnt, mps_len, &pth.clone())?;
            cur_cnt += mail_tray.mailpieces.len();
        }

        // // Fill in postage statement pdf.
        // let mut ps = PostageStatement::load_new().await?;
        // ps.fill_and_save(&mailing, pth.clone())?;

        // eprintln!("{} mailpieces", mailing.mailpieces.len());

        Ok(mailing)
    }
}

/// Pre-sort mail.
///
/// Determine barcode_id based on sort level.
pub fn presort_mailpieces(mut mailpieces: Vec<Mailpiece>) -> Vec<MailTray> {
    let mut ret = Vec::new();

    // Sort for chunking.
    mailpieces.sort_unstable_by_key(|o| o.zip5);

    let mut mixed_aadcs = Vec::with_capacity(mailpieces.len());
    for (key, chunk) in &mailpieces.into_iter().chunk_by(|mp| mp.zip5) {
        pub const PRESORT_MIN: usize = 200;
        let grp: Vec<Mailpiece> = chunk.collect();
        if grp.len() >= PRESORT_MIN {
            eprintln!("{key:05} {}", grp.len());
            ret.extend(segment_trays(BarcodeId::FiveDigit, grp));
        } else {
            mixed_aadcs.extend(grp);
        }
    }

    eprintln!("mixed aadc {}", mixed_aadcs.len());
    ret.extend(segment_trays(BarcodeId::MixedAadc, mixed_aadcs));

    // Set tray names.
    let mut chr = 'A' as u32;
    for (idx, tray) in ret.iter_mut().enumerate() {
        tray.name = char::from_u32(chr).expect("Invalid character").into();
        chr += 1;
    }

    ret
}

/// Segement pre-sorted groups into USPS trays.
pub fn segment_trays(barcode_id: BarcodeId, mailpieces: Vec<Mailpiece>) -> Vec<MailTray> {
    // 600 envelopes per 1ft tray.
    // Tray Length: 12 inches
    // Envelope Thickness: Varies, but a standard #10 envelope with a single sheet of paper is approximately 0.02 inches thick.
    // Fit Calculation
    // Number of Envelopes Lengthwise:
    // 12 inches ÷ 0.02 inches/envelope = 600 envelopes 12 inches ÷ 0.02 inches/envelope = 600 envelopes
    pub const CAP_1FOOT: usize = 600;
    pub const CAP_2FOOT: usize = 1200;

    // Place all trays in return list for naming "_tray1ofN".
    let mut ret = Vec::new();

    if mailpieces.len() <= CAP_1FOOT {
        ret.push(MailTray {
            name: "".into(),
            size: OneFoot,
            barcode_id,
            mailpieces,
        });
    } else if mailpieces.len() <= CAP_2FOOT {
        ret.push(MailTray {
            name: "".into(),
            size: TwoFoot,
            barcode_id,
            mailpieces,
        });
    } else {
        // Split mailpieces into 2-foot trays and remaining pieces.
        let mut remaining_pieces = mailpieces.as_slice();
        while remaining_pieces.len() > CAP_2FOOT {
            let (left, right) = remaining_pieces.split_at(CAP_2FOOT);
            ret.push(MailTray {
                name: "".into(),
                size: TraySize::TwoFoot,
                barcode_id,
                mailpieces: left.to_vec(),
            });
            remaining_pieces = right;
        }

        // Handle remaining pieces.
        if remaining_pieces.len() > CAP_1FOOT {
            ret.push(MailTray {
                name: "".into(),
                size: TraySize::TwoFoot,
                barcode_id,
                mailpieces: remaining_pieces.to_vec(),
            });
        } else if !remaining_pieces.is_empty() {
            ret.push(MailTray {
                name: "".into(),
                size: TraySize::OneFoot,
                barcode_id,
                mailpieces: remaining_pieces.to_vec(),
            });
        }
    }

    ret
}

/// A tray of mailpieces.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct MailTray {
    pub name: String,
    pub size: TraySize,
    pub barcode_id: BarcodeId,
    pub mailpieces: Vec<Mailpiece>,
}

impl MailTray {
    // See "Intelligent Mail Barcode Technical Resource Guide" PDF.
    // See https://postalpro.usps.com/node/221.
    pub async fn add_barcodes(&mut self, cur_cnt: usize, mps_len: f64) -> Result<bool> {
        let mut self_clone = self.clone();
        let mp_len = self.mailpieces.len() as f64;

        // Fetch barcode encoding for each mailpiece.
        let mut did_fetch = false;
        for (idx, mp) in self_clone
            .mailpieces
            .iter()
            .enumerate()
            .filter(|(_, mp)| mp.barcode.is_empty())
        // .take(1)
        {
            did_fetch = true;
            let pct = ((((cur_cnt + idx) as f64 + 1.0) / mps_len) * 100.0) as u8;
            eprintln!("  {}% {}", pct, mp);

            // Create routing code (zip + delivery point).
            // The Routing Code field is an optional field, which may contain a
            // 5-digit ZIP Code, a 9-digit ZIP+4 code, or an 11-digit delivery
            // point code. When used on letters for automation-rate eligibility purposes,
            // the routing code must contain a delivery point code from CASS-certified
            // software that accurately matches the delivery address.
            // From "Intelligent Mail Barcode Technical Resource Guide" PDF.
            // See https://postalpro.usps.com/node/221.
            let mut routing_code = if mp.zip4 != 0 {
                format!("{:05}{:04}", mp.zip5, mp.zip4)
            } else {
                format!("{:05}", mp.zip5)
            };
            if mp.zip4 != 0 {
                if let Some(delivery_point) = &mp.delivery_point {
                    routing_code.push_str(delivery_point);
                }
            }

            // eprintln!("  routing_code:{routing_code}");
            self.mailpieces[idx].barcode = encode_barcode(
                &format!("{}", self.barcode_id),
                STID_RSR,
                &CFG.mailer_id,
                &format!("{:06}", mp.id),
                &routing_code,
            )
            .await?;
        }

        Ok(did_fetch)
    }

    pub fn create_envelopes_letters<P>(&self, cur_cnt: usize, mps_len: f64, pth: P) -> Result<()>
    where
        P: AsRef<Path>,
    {
        // Read letter template from disk.
        let ltr_tmpl = letter_template()?;

        // 50 chunk size is based on capacity of an envelope printer and paper folding machine.
        const CHUNK_SIZE: usize = 50;
        let chunk_cnt = self
            .mailpieces
            .iter()
            .chunks(CHUNK_SIZE)
            .into_iter()
            .count();
        for (chunk_idx, chunk) in (&self.mailpieces.iter().enumerate().chunks(CHUNK_SIZE))
            .into_iter()
            .enumerate()
            // .take(1)
        {
            // Collect chunk to measure length.
            let chunk: Vec<_> = chunk.collect();
            let chunk_len = chunk.len();

            // Create letter name.
            let ltr_name = format!(
                "{}_{}of{:02}_cnt{}_ltr",
                self.name,
                chunk_idx + 1,
                chunk_cnt,
                chunk_len
            );
            // Create envelope name.
            let env_name = format!(
                "{}_{}of{:02}_cnt{}_env",
                self.name,
                chunk_idx + 1,
                chunk_cnt,
                chunk_len
            );
            eprintln!("creating {}", ltr_name);

            // Create a pdf document for multiple letters.
            let mut ltr = ltr_tmpl.clone_clear();

            // Create a pdf document for multiple envelopes.
            let mut env_doc = EnvelopeDocument::new(env_name);

            // Iterate through each mailpiece in the current chunk.
            for (mp_idx, mp) in chunk {
                let pct = ((((cur_cnt + mp_idx) as f64 + 1.0) / mps_len) * 100.0) as u8;
                eprintln!("  {}% {}", pct, mp);

                // Create envelope.
                env_doc.create_page(mp, mp_idx % CHUNK_SIZE == 0);

                // Create letter.
                // Clone letter template with text.
                let mut cur_ltr = ltr_tmpl.clone();
                // Replace placeholder text with actual name.
                cur_ltr.replace_par_at(0, "{{name}}", &mp.name);
                // Copy paragraphs to destination letter.
                ltr.copy_pars(cur_ltr.clone());
                // Add a page break.
                ltr.add_pag_brk();
            }

            // Create path.
            let mut pth = pth.as_ref().to_path_buf();

            // Save envelope document to disk.
            pth.push(env_doc.name);
            pth.set_extension("pdf");
            env_doc
                .doc
                .save(&mut BufWriter::new(File::create(&pth).unwrap()))?;

            // Save letter document to disk.
            pth.pop();
            pth.push(ltr_name);
            pth.set_extension("");
            ltr.save_pdf(&pth)?;
        }

        Ok(())
    }
}

pub fn letter_template() -> Result<Doc> {
    read_from_file::<Doc>(FLE_PTH_LTR)
}

pub fn mailing_cfg() -> Result<MailingCfg> {
    read_from_file::<MailingCfg>(FLE_PTH_CFG)
}

/// STID 301 is USPS Marketing Mail, Basic automation, No Address Corrections.
///
/// For use with USPS barcode.
///
/// See the Service Type IDentifier (STID) Table
/// https://postalpro.usps.com/mailing/service-type-identifiers.
pub const STID_NO_ADR: &str = "301";

/// STID 272 is USPS Marketing Mail, Basic automation, with Return Service Requested.
///
/// For use with USPS barcode.
///
/// See the Service Type IDentifier (STID) Table
/// https://postalpro.usps.com/mailing/service-type-identifiers.
pub const STID_RSR: &str = "272";

// USPS serial_id:
// The USPS Intelligent Mail Barcode (IMb) contains several components, one of which is the serial number. The serial number within the IMb can be used in different ways depending on the mailer's needs and USPS requirements. Here's how it works:
//
// Unique Serial Number Across Multiple Mailings
// Mailpiece Identifier (Serial Number): This part of the IMb is designed to help mailers uniquely identify individual mailpieces. The serial number can be unique to a single mailing or unique across multiple mailings, depending on the level of tracking and management the mailer requires.
// Purpose: The primary purpose of the serial number is to uniquely identify each mailpiece to facilitate tracking and ensure accurate delivery. It can also help in managing returns and tracking responses.

/// Custom envelope information.
#[derive(Debug, Default, Serialize, Deserialize, Clone)]
pub struct MailingCfg {
    pub mailer_id: String,
    pub crid: String,
    pub eps_id: String,
    pub nonprofit_auth_id: String,
    pub last_mailpiece_id: u32,
    pub indicia: Indicia,
    pub from: Mailpiece,
    pub ps: PostageStatementCfg,
}

#[derive(Debug, Default, Serialize, Deserialize, Clone)]
pub struct PostageStatementCfg {
    pub adr: Mailpiece,
    pub email: String,
    pub phone: String,
    pub post_office_mailing: String,
    pub mailing_date: String,
    pub last_statement_id: u16,
}

/// USPS barcode identifier.
/// From "Intelligent Mail Barcode Technical Resource Guide" PDF.
/// See https://postalpro.usps.com/node/221.
#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq, Eq)]
pub enum BarcodeId {
    Default,      // 00 - Default / No OEL Information
    CarrierRoute, // 10 - Carrier Route (CR), Enhanced Carrier Route (ECR), and FIRM
    FiveDigit,    // 20 - 5-Digit/Scheme
    ThreeDigit,   // 30 - 3-Digit/Scheme
    Aadc,         // 40 - Area Distribution Center (ADC)
    MixedAadc,    // 50 - Mixed Area Distribution Center (MADC), Origin Mixed ADC (OMX)
}
impl fmt::Display for BarcodeId {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            BarcodeId::Default => write!(f, "00"),
            BarcodeId::CarrierRoute => write!(f, "10"),
            BarcodeId::FiveDigit => write!(f, "20"),
            BarcodeId::ThreeDigit => write!(f, "30"),
            BarcodeId::Aadc => write!(f, "40"),
            BarcodeId::MixedAadc => write!(f, "50"),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub enum TraySize {
    OneFoot,
    TwoFoot,
}

#[derive(Debug)]
enum SortLvl {
    FiveDigit,                     // 5DIG - 5 Digit
    ThreeDigitColA,                // L002A - 3 Digit Col A
    ThreeDigitColB1,               // L002B1 - 3 Digit Col B
    ThreeDigitColB2,               // L002B2 - 3 Digit Col B
    ThreeDigitColC,                // L002C - 3 Digit Col C SCF
    ThreeDigitSchemeSortation, // L003 - 3 Digit Zip Code Prefix Groups - 3 Digit Scheme Sortation
    ADC3DigitSortationA,       // L004A - 3 Digit Zip Code Prefix Groups - ADC Sortation
    ADC3DigitSortationB,       // L004B - 3 Digit Zip Code Prefix Groups - ADC Sortation
    ADC3DigitSortationC,       // L004C - 3 Digit Zip Code Prefix Groups - ADC Sortation
    SCFSortation,              // L005 - 3 Digit Zip Code Prefix Groups - SCF Sortation
    FiveDigitScheme,           // L007 - 5-Digit Scheme
    MixedADCsA,                // L009A - Mixed ADCs
    MixedADCsB,                // L009B - Mixed ADCs
    NDCASFEntry,               // L010 - NDC/ASF Entry
    NonNDCASFEntryA,           // L011A - Non-NDC/ASF Entry
    NonNDCASFEntryB,           // L011B - Non-NDC/ASF Entry
    FiveDigitZIPSchemeCombination, // L012 - 5-Digit ZIP Scheme Combination
    ADCOptionalSortation, // L015 - 3-Digit ZIP Code Prefix Groups - ADC Package Services Optional Sortation
    Omx,                  // L201A - Periodicals Origin Mixed ADC (OMX)
    MixedAADC,            // L201B - First-Class Mail Mixed AADC
    NDCs,                 // L601 - Network Distribution Centers (NDCs)
    ASFs,                 // L602 - ASFs
    AADCLetterSizeMailingsA, // L801A - AADCs - Letter-Size Mailings
    AADCLetterSizeMailingsB, // L801B - AADCs - Letter-Size Mailings
    None,                 // No value selected
}
