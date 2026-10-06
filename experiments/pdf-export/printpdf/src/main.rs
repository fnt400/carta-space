use std::{collections::BTreeMap, error::Error, fs};

use printpdf::{
    Mm, PdfDocument, PdfSaveOptions,
    html::{PageMargins, XmlRenderOptions, add_xml_to_document, build_font_pool},
};

fn main() -> Result<(), Box<dyn Error>> {
    let args: Vec<_> = std::env::args_os().collect();
    if args.len() != 3 {
        return Err("usage: carta-printpdf-bench INPUT.html OUTPUT.pdf".into());
    }
    let mut html = fs::read_to_string(&args[1])?;
    let style = format!("<style>{}</style>", include_str!("../classic.css"));
    // Append the fixed publication profile after any input stylesheet.
    if let Some(offset) = html.rfind("</head>") {
        html.insert_str(offset, &style);
    } else {
        html.insert_str(0, &style);
    }
    let fonts: BTreeMap<String, Vec<u8>> = [
        (
            "Source Serif 4",
            include_bytes!("../../fonts/SourceSerif4-Regular.ttf").as_slice(),
        ),
        (
            "Source Serif 4 Italic",
            include_bytes!("../../fonts/SourceSerif4-Italic.ttf").as_slice(),
        ),
        (
            "Source Serif 4 Semibold",
            include_bytes!("../../fonts/SourceSerif4-Semibold.ttf").as_slice(),
        ),
        (
            "Source Code Pro",
            include_bytes!("../../fonts/SourceCodePro-Regular.ttf").as_slice(),
        ),
        (
            "Noto Sans",
            include_bytes!("../../fonts/NotoSans-Regular.ttf").as_slice(),
        ),
        (
            "Noto Sans Hebrew",
            include_bytes!("../../fonts/NotoSansHebrew-Regular.ttf").as_slice(),
        ),
        (
            "Noto Sans Arabic",
            include_bytes!("../../fonts/NotoSansArabic-Regular.ttf").as_slice(),
        ),
        (
            "Noto Sans Devanagari",
            include_bytes!("../../fonts/NotoSansDevanagari-Regular.ttf").as_slice(),
        ),
        (
            "Noto Sans JP",
            include_bytes!("../../fonts/NotoSansJP-Regular.ttf").as_slice(),
        ),
    ]
    .into_iter()
    .map(|(name, bytes)| (name.to_string(), bytes.to_vec()))
    .collect();
    let font_bytes: usize = fonts.values().map(Vec::len).sum();
    // Exclude installed fonts from resolution to test embedded-font fallback.
    let font_pool = build_font_pool(&fonts, Some(&[]));
    let options = XmlRenderOptions {
        fonts,
        font_pool: Some(font_pool),
        page_width: Mm(210.0),
        page_height: Mm(297.0),
        margins: PageMargins::symmetric(Mm(25.0), Mm(32.0)),
        show_page_numbers: true,
        ..Default::default()
    };
    let mut doc = PdfDocument::new("Carta Classic printpdf experiment");
    add_xml_to_document(&mut doc, &html, &options)
        .map_err(|warnings| format!("HTML rendering failed: {warnings:?}"))?;
    let mut warnings = Vec::new();
    let bytes = doc.save(&PdfSaveOptions::default(), &mut warnings);
    fs::write(&args[2], &bytes)?;
    eprintln!(
        "pages={} pdf_bytes={} embedded_font_input_bytes={font_bytes}",
        doc.pages.len(),
        bytes.len()
    );
    for warning in warnings {
        eprintln!("{warning:?}");
    }
    Ok(())
}
