use crate::core::*;
use crate::models::*;
use crate::prsr::*;
use crate::usps::*;
use anyhow::{anyhow, Result};
use printpdf::path::{PaintMode, WindingOrder};
use printpdf::{BuiltinFont, Mm, PdfDocument};
use printpdf::{Color, Line, Point, Rect, Rgb};
use serde::{Deserialize, Serialize};
use std::fs::File;
use std::io::{BufReader, BufWriter, Cursor};

const FLE_PTH: &str = "mailing.json";
const FLE_PTH_CFG: &str = "mailing_cfg.json";

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Mailing {
    pub mailpieces: Vec<MailPiece>,
}

impl Mailing {
    pub fn new() -> Self {
        Self {
            mailpieces: Vec::new(),
        }
    }

    pub async fn load(pers: &mut [Person]) -> Result<Mailing> {
        // Read file from disk.
        let mut mailing = match read_from_file::<Mailing>(FLE_PTH) {
            Ok(mailing_from_disk) => mailing_from_disk,
            Err(_) => {
                let mut mailing = Mailing::new();

                // Get envelope data.
                let cfg = &mailing_cfg()?;

                // Calculate current serial id based on current mailing
                // and previous mailing. Each envelope gets a unique id.
                // Used in address barcode.
                let mut id = cfg.last_mailpiece_id;

                // Create mailpieces for each person.
                let adr_cnt = pers.iter().map(|p| p.adr_len()).sum::<usize>();
                mailing.mailpieces = Vec::with_capacity(adr_cnt);
                for per in pers.iter() {
                    if let Some(adrs) = &per.adrs {
                        for adr in adrs {
                            // See guidelines https://about.usps.com/publications/pub28/28c2_007.htm.
                            id += 1;
                            let mp = MailPiece {
                                name: dot_remove(per.name.clone()).to_uppercase(),
                                title1: string_to_opt(per.title1.clone()),
                                title2: string_to_opt(per.title2.clone()),
                                address1: adr.address1.clone(),
                                city: adr.city.clone(),
                                state: adr.state.clone(),
                                zip: adr.zip.clone(),
                                delivery_point: adr.delivery_point.clone(),
                                barcode_fadt: "".into(),
                                id,
                            };
                            mailing.mailpieces.push(mp);
                        }
                    } else {
                        return Err(anyhow!("missing address for {}", per));
                    }
                }

                // Write file to disk.
                write_to_file(&mailing, FLE_PTH)?;

                mailing
            }
        };

        // // Find longest title1.
        // pers.sort_unstable_by_key(|k| k.title1.len());
        // eprintln!("title1:{}", pers[pers.len() - 1].title1);

        // // Find longest address1.
        // mailpieces.sort_unstable_by_key(|k| k.address1.len());
        // eprintln!("address1:{}", mailpieces[mailpieces.len() - 1].address1);

        // TODO: SORT FOR USPS PRE-SORT DISCOUNT.

        // TODO: DETERMINE BARCODE_ID BASED ON SORT LEVEL
        // From: Intelligent Mail Barcode Technical Resource Guide
        // See: https://postalpro.usps.com/node/221
        //
        // Barcode Identifier
        // Value
        // Barcode ID / Optional Endorsement Line (OEL) Description
        // 00           Default / No OEL Information
        // 10           Carrier Route (CR), Enhanced Carrier Route (ECR), and FIRM
        // 20           5-Digit/Scheme
        // 30           3-Digit/Scheme
        // 40           Area Distribution Center (ADC)
        // 50           Mixed Area Distribution Center (MADC), Origin Mixed ADC (OMX)
        let barcode_todo = String::from("50");

        // Get envelope data.
        let cfg = &mailing_cfg()?;

        // Add barcode to mailpieces.
        // barcode_id: Uses pre-sort identifier.
        // serial_id: A sequential identifier within the mailing.
        mailing.add_barcodes_fadt(barcode_todo.clone(), cfg).await?;

        // Create envelopes
        mailing.create_envelopes(cfg)?;

        // TODO: CREATE LETTERS

        eprintln!("{} mailpieces", mailing.mailpieces.len());

        Ok(mailing)
    }

    pub async fn add_barcodes_fadt(
        &mut self,
        barcode_todo: String,
        cfg: &MailingCfg,
    ) -> Result<()> {
        // Clone self for file writing.
        let mut self_clone = self.clone();
        let mp_len = self.mailpieces.len() as f64;

        // Use the index as the serial number.
        for (idx, mp) in self_clone
            .mailpieces
            .iter()
            .enumerate()
            .filter(|(_, mp)| mp.barcode_fadt.is_empty())
            .take(1)
        {
            let pct = (((idx as f64 + 1.0) / mp_len) * 100.0) as u8;
            eprintln!("  {}% {} {}", pct, idx, mp);

            // Create routing code (zip + delivery point).
            let mut routing_code = mp.zip.replace('-', "");
            if let Some(delivery_point) = &mp.delivery_point {
                routing_code.push_str(delivery_point);
            }
            // eprintln!("  routing_code:{routing_code}");
            self.mailpieces[idx].barcode_fadt = encode_barcode_fadt(
                &barcode_todo,
                STID,
                &cfg.mailer_id,
                &format!("{:06}", mp.id),
                &routing_code,
            )
            .await?;

            // Checkpoint save.
            // Write intermediate file to disk.
            write_to_file(&self, FLE_PTH)?;
        }

        Ok(())
    }

    pub fn create_envelopes(&mut self, cfg: &MailingCfg) -> Result<()> {
        // Clone self for file writing.
        let mut self_clone = self.clone();
        let mp_len = self.mailpieces.len() as f64;

        // Use the index as the serial number.
        for (idx, mp) in self_clone
            .mailpieces
            .iter()
            .enumerate()
            // .filter(|(_, mp)| mp.barcode_fadt.is_empty())
            .take(1)
        {
            let pct = (((idx as f64 + 1.0) / mp_len) * 100.0) as u8;
            eprintln!("  {}% {} {}", pct, idx, mp);

            // TODOO: 50 ENVELOPES PER DOCUMENT
            create_envelope(mp, cfg)?;

            // TODO: 50 LETTERS PER DOCUMENT
        }

        // TODO: DETERMINE FILE NAME
        // TODO: DETERMINE FILE DIRECTORY

        Ok(())
    }
}

/// Creates an envelope in PDF format.
pub fn create_envelope(to: &MailPiece, cfg: &MailingCfg) -> Result<()> {
    // A Number 10 envelope, commonly used for business and personal correspondence,
    // has dimensions of 241.3 mm in width, and 104.8 mm in height.
    // Common envelope margins for printing can vary depending on the specific printer
    // and the design requirements, but here are some general guidelines that are
    // typically used:
    //  * Top Margin: 10-15 mm
    //  * Bottom Margin: 10-15 mm
    //  * Left Margin: 10-15 mm
    //  * Right Margin: 10-15 mm
    let width = Mm(241.3);
    let height = Mm(104.8);

    // Setup envelope.
    let (doc, page1, layer1) = PdfDocument::new("envelope", width, height, "FROM");
    let lyr_from = doc.get_page(page1).get_layer(layer1);

    // Setup font.
    let font = doc.add_builtin_font(BuiltinFont::Helvetica).unwrap();
    // current_layer.set_word_spacing(3000.0);
    // current_layer.set_character_spacing(10.0);

    // Write "from" address on envelope.
    // Return Address Placement:
    // The return address (sender's address) should be placed in the
    // upper left corner of the envelope within the area starting:
    //  * 15 mm from the left edge of the envelope.
    //  * 15 mm from the top edge of the envelope.
    let margin_from = Mm(10.0);
    lyr_from.begin_text_section();
    lyr_from.set_font(&font, 10.0);
    lyr_from.set_text_cursor(margin_from, height - margin_from);
    lyr_from.set_line_height(12.0);
    lyr_from.write_text(cfg.from.name.clone(), &font);
    lyr_from.add_line_break();
    lyr_from.write_text(cfg.from.address1.clone(), &font);
    lyr_from.add_line_break();
    lyr_from.write_text(
        format!("{}  {}  {}", cfg.from.city, cfg.from.state, cfg.from.zip),
        &font,
    );
    lyr_from.end_text_section();

    // Write "to" address on envelope.
    // Address Block Placement:
    // The address block (including the recipient's name, street address,
    // city, state, and ZIP Code) should be placed within the area starting:
    //  * 40 mm from the left edge of the envelope.
    //  * 60 mm from the bottom edge of the envelope.
    //  * 80 mm from the right edge of the envelope.
    //  * 40 mm from the top edge of the envelope.
    // Add layers for use in Adobe Illustrator.
    let lyr_to = doc.get_page(page1).add_layer("TO");
    let margin_to_x = Mm(85.0);
    let margin_to_y = Mm(45.0);
    lyr_to.begin_text_section();
    lyr_to.set_font(&font, 12.0);
    lyr_to.set_text_cursor(margin_to_x, height - margin_to_y);
    lyr_to.set_line_height(18.0);
    lyr_to.write_text(to.name.clone(), &font);
    lyr_to.add_line_break();
    if to.title1.is_some() {
        lyr_to.write_text(to.title1.clone().unwrap(), &font);
        lyr_to.add_line_break();
    }
    if to.title2.is_some() {
        lyr_to.write_text(to.title2.clone().unwrap(), &font);
        lyr_to.add_line_break();
    }
    lyr_to.write_text(to.address1.clone(), &font);
    lyr_to.add_line_break();
    lyr_to.write_text(format!("{}  {}  {}", to.city, to.state, to.zip), &font);
    lyr_to.add_line_break();
    // Write barcode.
    // See USPS guidelines https://pe.usps.com/text/qsg300/Q201a.htm.
    let mut rdr = Cursor::new(include_bytes!("../fonts/USPSIMBStandard.ttf").as_ref());
    let barcode_font = doc.add_external_font(&mut rdr).unwrap();
    lyr_to.set_font(&barcode_font, 16.0);
    lyr_to.write_text(to.barcode_fadt.clone(), &barcode_font);
    lyr_to.end_text_section();

    // Write a permit indicia.
    let lyr_indicia = doc.get_page(page1).add_layer("INDICIA");
    let margin_indicia_x = Mm(34.0);
    let margin_indicia_y = Mm(9.0);
    lyr_indicia.begin_text_section();
    lyr_indicia.set_font(&font, 8.0);
    lyr_indicia.set_text_cursor(width - margin_indicia_x, height - margin_indicia_y);
    lyr_indicia.set_line_height(10.0);
    lyr_indicia.write_text("NONPROFIT", &font);
    lyr_indicia.add_line_break();
    lyr_indicia.write_text("PRSRT MKTG", &font);
    lyr_indicia.add_line_break();
    lyr_indicia.write_text("AUTO", &font);
    lyr_indicia.add_line_break();
    lyr_indicia.write_text("U.S. POSTAGE PAID", &font);
    lyr_indicia.add_line_break();
    lyr_indicia.write_text(cfg.indicia.city_state.clone(), &font);
    lyr_indicia.add_line_break();
    lyr_indicia.write_text(format!("PERMIT NO. {}", cfg.indicia.permit_id), &font);
    lyr_indicia.end_text_section();
    // Draw rectangular outline around the indicia.
    let ll_x = width - margin_indicia_x - Mm(2.0);
    let ll_y = height - margin_indicia_y - Mm(20.0);
    let ur_x = width - Mm(5.0);
    let ur_y = height - Mm(5.0);
    let rect = Rect::new(ll_x, ll_y, ur_x, ur_y).with_mode(PaintMode::Stroke);
    lyr_indicia.add_rect(rect);

    doc.save(&mut BufWriter::new(
        File::create("test_envelope.pdf").unwrap(),
    ))?;

    Ok(())
}

pub fn mailing_cfg() -> Result<MailingCfg> {
    read_from_file::<MailingCfg>(FLE_PTH_CFG)
}

/// A permit indicia's unique information.
#[derive(Debug, Default, Serialize, Deserialize, Clone)]
pub struct Indicia {
    pub city_state: String,
    pub permit_id: String,
}

/// STID 301 is USPS Marketing Mail, Basic automation, No Address Corrections.
///
/// For use with USPS barcode.
///
/// See the Service Type IDentifier (STID) Table
/// https://postalpro.usps.com/mailing/service-type-identifiers.
pub const STID: &str = "301";

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
    pub last_mailpiece_id: u32,
    pub indicia: Indicia,
    pub from: MailPiece,
}
