use crate::core::*;
use crate::mailing::*;
use crate::models::*;
use crate::prsr::*;
use path::PaintMode;
use printpdf::*;
use serde::Deserialize;
use serde::Serialize;

const LYR_FROM: &str = "FROM";
const WIDTH: Mm = Mm(241.3);
const HEIGHT: Mm = Mm(104.8);

static FNT_IMB: &[u8] = include_bytes!("../fonts/USPSIMBStandard.ttf");

pub struct EnvelopeDocument {
    pub name: String,
    pub doc: PdfDocumentReference,
    pub font: IndirectFontRef,
    pub font_barcode: IndirectFontRef,
    pub pg_idx1: PdfPageIndex,
    pub lyr_idx1: PdfLayerIndex,
}

impl EnvelopeDocument {
    pub fn new(name: String) -> Self {
        // Setup document.
        // A Number 10 envelope, commonly used for business and personal correspondence,
        // has dimensions of 241.3 mm in width, and 104.8 mm in height.
        // Common envelope margins for printing can vary depending on the specific printer
        // and the design requirements, but here are some general guidelines that are
        // typically used:
        //  * Top Margin: 10-15 mm
        //  * Bottom Margin: 10-15 mm
        //  * Left Margin: 10-15 mm
        //  * Right Margin: 10-15 mm

        let (doc, pg_idx1, lyr_idx1) = PdfDocument::new(&name, WIDTH, HEIGHT, LYR_FROM);

        // Setup fonts.
        let font = doc.add_builtin_font(BuiltinFont::Helvetica).unwrap();
        let font_barcode = doc.add_external_font(FNT_IMB).unwrap();

        Self {
            name,
            doc,
            font,
            font_barcode,
            pg_idx1,
            lyr_idx1,
        }
    }

    /// Create one envelope per page.
    pub fn create_page(&mut self, to: &Mailpiece, is_pg1: bool) {
        // Create envelope page.
        let (pg_idx, lyr_idx) = if is_pg1 {
            (self.pg_idx1, self.lyr_idx1)
        } else {
            self.doc.add_page(WIDTH, HEIGHT, LYR_FROM)
        };

        // Offset X for printer quirk.
        let offset = Mm(0.0);

        // Get "FROM" layer.
        let lyr_from = self.doc.get_page(pg_idx).get_layer(lyr_idx);

        // Write "from" address on envelope.
        // Return Address Placement:
        // The return address (sender's address) should be placed in the
        // upper left corner of the envelope within the area starting:
        //  * 15 mm from the left edge of the envelope.
        //  * 15 mm from the top edge of the envelope.
        let margin_from = Mm(10.0);
        lyr_from.begin_text_section();
        lyr_from.set_font(&self.font, 10.0);
        lyr_from.set_text_cursor(margin_from + offset, HEIGHT - margin_from);
        lyr_from.set_line_height(12.0);
        lyr_from.write_text(CFG.from.name.clone(), &self.font);
        lyr_from.add_line_break();
        lyr_from.write_text(CFG.from.address1.clone(), &self.font);
        lyr_from.add_line_break();
        lyr_from.write_text(
            format!(
                "{}  {}  {:05}-{:04}",
                CFG.from.city, CFG.from.state, CFG.from.zip5, CFG.from.zip4
            ),
            &self.font,
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
        let lyr_to = self.doc.get_page(pg_idx).add_layer("TO");
        let margin_to_x = Mm(85.0) + offset;
        let margin_to_y = Mm(45.0);
        lyr_to.begin_text_section();
        lyr_to.set_font(&self.font, 12.0);
        lyr_to.set_text_cursor(margin_to_x, HEIGHT - margin_to_y);
        lyr_to.set_line_height(18.0);
        lyr_to.write_text(dot_remove(to.name.clone()).to_uppercase(), &self.font);
        lyr_to.add_line_break();
        if to.title1.is_some() {
            lyr_to.write_text(to.title1.clone().unwrap(), &self.font);
            lyr_to.add_line_break();
        }
        if to.title2.is_some() {
            lyr_to.write_text(to.title2.clone().unwrap(), &self.font);
            lyr_to.add_line_break();
        }
        lyr_to.write_text(to.address1.clone(), &self.font);
        lyr_to.add_line_break();
        lyr_to.write_text(
            format!("{}  {}  {:05}-{:04}", to.city, to.state, to.zip5, to.zip4),
            &self.font,
        );
        lyr_to.add_line_break();
        // Write barcode.
        // See USPS guidelines https://pe.usps.com/text/qsg300/Q201a.htm.
        lyr_to.set_font(&self.font_barcode, 16.0);
        lyr_to.write_text(to.barcode.clone(), &self.font_barcode);
        lyr_to.end_text_section();

        // // Write a permit indicia.
        // let lyr_indicia = self.doc.get_page(pg_idx).add_layer("INDICIA");
        // let margin_indicia_x = Mm(34.0);
        // let margin_indicia_y = Mm(9.0);
        // lyr_indicia.begin_text_section();
        // lyr_indicia.set_font(&self.font, 8.0);
        // lyr_indicia.set_text_cursor(WIDTH - margin_indicia_x, HEIGHT - margin_indicia_y);
        // lyr_indicia.set_line_height(10.0);
        // lyr_indicia.write_text("NONPROFIT", &self.font);
        // lyr_indicia.add_line_break();
        // lyr_indicia.write_text("PRSRT MKTG", &self.font);
        // lyr_indicia.add_line_break();
        // lyr_indicia.write_text("AUTO", &self.font);
        // lyr_indicia.add_line_break();
        // lyr_indicia.write_text("U.S. POSTAGE PAID", &self.font);
        // lyr_indicia.add_line_break();
        // lyr_indicia.write_text(CFG.indicia.city_state.clone(), &self.font);
        // lyr_indicia.add_line_break();
        // lyr_indicia.write_text(format!("PERMIT NO. {}", CFG.indicia.permit_id), &self.font);
        // lyr_indicia.end_text_section();
        // // Draw rectangular outline around the indicia.
        // let ll_x = WIDTH - margin_indicia_x - Mm(2.0);
        // let ll_y = HEIGHT - margin_indicia_y - Mm(20.0);
        // let ur_x = WIDTH - Mm(5.0);
        // let ur_y = HEIGHT - Mm(5.0);
        // let rect = Rect::new(ll_x, ll_y, ur_x, ur_y).with_mode(PaintMode::Stroke);
        // lyr_indicia.add_rect(rect);

        // Write "Return Service Requested".
        let lyr_rsr = self.doc.get_page(pg_idx).add_layer("RSR");
        let margin_rsr_x = Mm(37.0);
        let margin_rsr_y = Mm(30.0);
        lyr_rsr.begin_text_section();
        lyr_rsr.set_font(&self.font, 8.0);
        lyr_rsr.set_text_cursor(WIDTH - margin_rsr_x, HEIGHT - margin_rsr_y);
        lyr_rsr.write_text("Return Service Requested", &self.font);
        lyr_rsr.end_text_section();
    }
}

/// A permit indicia's unique information.
#[derive(Debug, Default, Serialize, Deserialize, Clone)]
pub struct Indicia {
    pub city_state: String,
    pub permit_id: String,
}
