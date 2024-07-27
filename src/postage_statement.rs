use std::path::{Path, PathBuf};

use anyhow::{anyhow, Result};
use lopdf::{dictionary, Document, Object, ObjectId, Stream};

use crate::{fetch_pdf, numfmt, Mailing, CFG};

/// Struct representing a PDF document.
pub struct PostageStatement {
    doc: Document,
    font_id: Option<ObjectId>,
}

impl PostageStatement {
    /// Creates a new `PostageStatement` instance by loading the document from the specified path.
    ///
    /// # Arguments
    /// * `pth` - The path to the input PDF document.
    ///
    /// # Returns
    /// A `PostageStatement` instance.
    pub fn new<P>(pth: P) -> Result<Self>
    where
        P: AsRef<Path>,
    {
        let doc = Document::load(pth)?;
        Ok(PostageStatement { doc, font_id: None })
    }

    /// Loads a new Postage Statement document from a PDF file.
    ///
    /// This function can be used to load a Postage Statement document from a local file or a remote URL.
    ///
    /// # Returns
    /// A `PostageStatement` instance if the loading is successful, otherwise an error.
    pub async fn load_new() -> Result<Self> {
        let pth = fetch_pdf("https://about.usps.com/forms/ps3602n.pdf").await?;
        Self::new(pth)
    }

    /// Fill in the postage statement and save the file.
    pub fn fill_and_save(&mut self, mailing: &Mailing, mut pth: PathBuf) -> Result<()> {
        // Get page IDs.
        let pg1_id = self.get_page_id(0)?;
        let pg2_id = self.get_page_id(1)?;

        // Add address.
        let mut fnt_sze = 9.0;
        let x = 60.0;
        let mut y = 698.0;
        let y_dlt = fnt_sze + (0.2 * fnt_sze);
        self.add_text_to_pdf(pg1_id, &CFG.ps.adr.name, x, y, fnt_sze)?;
        y -= y_dlt;
        self.add_text_to_pdf(pg1_id, &CFG.ps.adr.address1, x, y, fnt_sze)?;
        y -= y_dlt;
        self.add_text_to_pdf(
            pg1_id,
            &format!(
                "{}, {} {}-{}",
                &CFG.ps.adr.city, &CFG.ps.adr.state, &CFG.ps.adr.zip5, &CFG.ps.adr.zip4
            ),
            x,
            y,
            fnt_sze,
        )?;

        // Add email and phone.
        let x = 170.0;
        let mut y = 698.0;
        fnt_sze = 8.0;
        self.add_text_to_pdf(pg1_id, &CFG.ps.email, x, y, fnt_sze)?;
        y -= y_dlt;
        self.add_text_to_pdf(pg1_id, &CFG.ps.phone, x, y, fnt_sze)?;

        // Add nonprofit auth.
        let x = 188.0;
        let y = 666.0;
        fnt_sze = 9.0;
        self.add_text_to_pdf(pg1_id, &CFG.nonprofit_auth_id, x, y, fnt_sze)?;

        // Add EPS account number..
        let x = 122.0;
        let y = 648.0;
        fnt_sze = 9.0;
        self.add_text_to_pdf(pg1_id, &CFG.eps_id, x, y, fnt_sze)?;

        // Add CRID.
        let x = 210.0;
        let y = 648.0;
        fnt_sze = 9.0;
        self.add_text_to_pdf(pg1_id, &CFG.crid, x, y, fnt_sze)?;

        // Post Office of Mailing.
        let x = 60.0;
        let y = 620.0;
        fnt_sze = 9.0;
        self.add_text_to_pdf(pg1_id, &CFG.ps.post_office_mailing, x, y, fnt_sze)?;

        // Mailing Date.
        let x = 185.0;
        let y = 620.0;
        fnt_sze = 9.0;
        self.add_text_to_pdf(pg1_id, &CFG.ps.mailing_date, x, y, fnt_sze)?;

        // Total # of Pieces.
        let x = 310.0;
        let y = 595.1;
        fnt_sze = 9.0;
        self.add_text_to_pdf(
            pg1_id,
            &numfmt(mailing.mailpiece_cnt as usize),
            x,
            y,
            fnt_sze,
        )?;

        // Statement Seq. No.
        let x = 365.0;
        let y = 620.0;
        fnt_sze = 9.0;
        self.add_text_to_pdf(
            pg1_id,
            &format!("{:03}", CFG.ps.last_statement_id + 1),
            x,
            y,
            fnt_sze,
        )?;

        // 1 ft. Letter Trays.
        let x = 529.0;
        let y = 597.0;
        fnt_sze = 9.0;
        self.add_text_to_pdf(pg1_id, &mailing.tray_1ft_cnt.to_string(), x, y, fnt_sze)?;

        // 1 ft. Letter Trays.
        let y = 573.0;
        fnt_sze = 9.0;
        self.add_text_to_pdf(pg1_id, &mailing.tray_2ft_cnt.to_string(), x, y, fnt_sze)?;

        // Permit Imprint ID.
        let x = 365.0;
        let y = 571.0;
        fnt_sze = 9.0;
        self.add_text_to_pdf(pg1_id, &CFG.indicia.permit_id, x, y, fnt_sze)?;

        // Type of Postage.
        let x = 56.0;
        let y = 595.1;
        fnt_sze = 12.0;
        self.add_text_to_pdf(pg1_id, "X", x, y, fnt_sze)?;

        // Processing Category.
        let x = 130.5;
        let y = 595.1;
        fnt_sze = 12.0;
        self.add_text_to_pdf(pg1_id, "X", x, y, fnt_sze)?;

        // Move Update Method.
        let x = 130.5;
        let y = 518.0;
        fnt_sze = 12.0;
        self.add_text_to_pdf(pg1_id, "X", x, y, fnt_sze)?;

        // Combined Mailing.
        let x = 130.5;
        let y = 483.5;
        fnt_sze = 12.0;
        self.add_text_to_pdf(pg1_id, "X", x, y, fnt_sze)?;

        // Part A.
        let x = 181.5;
        let y = 471.0;
        fnt_sze = 12.0;
        self.add_text_to_pdf(pg1_id, "X", x, y, fnt_sze)?;

        // Three Nos.
        let x = 414.1;
        let y = 512.0;
        fnt_sze = 12.0;
        self.add_text_to_pdf(pg1_id, "X", x, y, fnt_sze)?;
        let y = 498.0;
        self.add_text_to_pdf(pg1_id, "X", x, y, fnt_sze)?;
        let y = 485.0;
        self.add_text_to_pdf(pg1_id, "X", x, y, fnt_sze)?;

        // Page two.

        // // 5-digit No. of Pieces.
        // let x = 181.5;
        // let y = 471.0;
        // fnt_sze = 12.0;
        // self.add_text_to_pdf(pg2_id, "X", x, y, fnt_sze)?;


        pth.push("_postage_statement");
        pth.set_extension("pdf");
        self.save(pth);

        Ok(())
    }

    /// Gets the page ID of the page at the specified index.
    ///
    /// # Arguments
    /// * `index` - The index of the page (0-based).
    ///
    /// # Returns
    /// The page ID.
    pub fn get_page_id(&self, index: usize) -> Result<ObjectId> {
        let pages = self.doc.get_pages();
        let (_, &page_id) = pages
            .iter()
            .nth(index)
            .ok_or(anyhow!("Page index out of bounds"))?;
        Ok(page_id)
    }

    /// Adds text to the specified page of the PDF document at the given coordinates with the specified font size.
    ///
    /// # Arguments
    /// * `page_id` - The ID of the page to which the text will be added.
    /// * `text` - The text to add to the page.
    /// * `x` - The x-coordinate for the text placement.
    /// * `y` - The y-coordinate for the text placement.
    /// * `font_size` - The font size of the text.
    pub fn add_text_to_pdf(
        &mut self,
        page_id: ObjectId,
        text: &str,
        x: f32,
        y: f32,
        font_size: f32,
    ) -> Result<()> {
        // Ensure the font is added to the document only once
        if self.font_id.is_none() {
            let font_id = self.doc.add_object(dictionary! {
                "Type" => "Font",
                "Subtype" => "Type1",
                "BaseFont" => "Helvetica",
            });
            self.font_id = Some(font_id);
        }
        let font_id = self.font_id.unwrap();

        // Create a new content stream with the text to add
        let content = format!("BT /F1 {} Tf {} {} Td ({}) Tj ET", font_size, x, y, text);
        let new_content_stream = Stream::new(dictionary! {}, content.as_bytes().to_vec());
        let new_content_id = self.doc.add_object(new_content_stream);

        // Retrieve the existing content streams
        let page = self.doc.get_object_mut(page_id)?.as_dict_mut()?;
        let existing_contents = page.get(b"Contents")?;

        // Combine the existing content streams with the new content stream
        let combined_contents = match existing_contents {
            Object::Array(array) => {
                let mut new_array = array.clone();
                new_array.push(Object::Reference(new_content_id));
                Object::Array(new_array)
            }
            Object::Reference(id) => Object::Array(vec![
                Object::Reference(*id),
                Object::Reference(new_content_id),
            ]),
            _ => Object::Reference(new_content_id),
        };

        // Update the page dictionary with the combined content streams
        page.set("Contents", combined_contents);

        // Ensure the font is added to the resources only once
        let resources = page.get_mut(b"Resources")?.as_dict_mut()?;
        if let Ok(fonts) = resources.get_mut(b"Font") {
            if let Ok(fonts_dict) = fonts.as_dict_mut() {
                fonts_dict.set("F1", Object::Reference(font_id));
            }
        } else {
            resources.set(
                "Font",
                dictionary! {
                    "F1" => Object::Reference(font_id),
                },
            );
        }

        Ok(())
    }

    /// Saves the PDF document to the specified output path.
    ///
    /// # Arguments
    /// * `pth` - The path to save the modified PDF document.
    pub fn save<P>(&mut self, pth: P) -> Result<(), Box<dyn std::error::Error>>
    where
        P: AsRef<Path>,
    {
        self.doc.save(pth)?;
        Ok(())
    }
}
