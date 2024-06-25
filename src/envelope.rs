use printpdf::{BuiltinFont, Mm, PdfDocument};
use std::{fs::File, io::BufWriter};

// A Number 10 envelope, commonly used for business and personal correspondence,
// has dimensions of 241.3 mm in width, and 104.8 mm in height.
//
// Common envelope margins for printing can vary depending on the specific printer
// and the design requirements, but here are some general guidelines that are
// typically used:
//  * Top Margin: 10-15 mm
//  * Bottom Margin: 10-15 mm
//  * Left Margin: 10-15 mm
//  * Right Margin: 10-15 mm

/// Creates an envelope PDF.
pub fn create_envelope() {
    let width = Mm(241.3);
    let height = Mm(104.8);
    let margin = Mm(10.0);
    let (doc, page1, layer1) = PdfDocument::new("envelope_1", width, height, "Layer 1");
    let current_layer = doc.get_page(page1).get_layer(layer1);

    // Setup font.
    let font = doc.add_builtin_font(BuiltinFont::Helvetica).unwrap();

    // current_layer.set_word_spacing(3000.0);
    // current_layer.set_character_spacing(10.0);

    let text1 = "LOREM IPSUM";
    let text2 = "DOLOR, SIT AMET";
    current_layer.begin_text_section();

    current_layer.set_font(&font, 10.0);
    current_layer.set_text_cursor(margin, height - margin);
    current_layer.set_line_height(12.0);

    current_layer.write_text(text1, &font);
    current_layer.add_line_break();
    current_layer.write_text(text2, &font);

    current_layer.end_text_section();

    let text3 = "Lorem ipsum";

    // current_layer.use_text(text3, 12.0, margin, margin, &font);

    doc.save(&mut BufWriter::new(
        File::create("test_envelope.pdf").unwrap(),
    ))
    .unwrap();
}
