use pulldown_cmark::{Event, HeadingLevel, Parser, Tag};
use std::env;
use std::error::Error;
use std::ffi::{OsStr, OsString};
use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

const CARTA_CLASSIC: &str = r#"
#set page(
  paper: "a4",
  margin: (x: 32mm, y: 25mm),
  header: none,
  footer: context align(center, text(size: 8pt, counter(page).display("1"))),
  footer-descent: 10mm,
)
#set text(
  font: ("Source Serif 4", "Noto Sans", "Noto Sans Hebrew",
         "Noto Sans Arabic", "Noto Sans Devanagari", "Noto Sans JP"),
  size: 11pt,
  fill: black,
  hyphenate: false,
  top-edge: 0.8em,
  bottom-edge: -0.2em,
)
#set par(justify: false, leading: 0.35em, spacing: 0.85em)
#set align(left)
#set heading(numbering: none)
#show heading: set text(weight: "semibold")
#show heading.where(level: 1): set text(size: 20pt)
#show heading.where(level: 2): set text(size: 15pt)
#show heading.where(level: 3): set text(size: 12pt)
#show strong: set text(weight: "semibold")
#set quote(block: true, quotes: false)
#show quote: it => block(inset: (left: 8mm), it.body)
#set list(indent: 2mm, body-indent: 5mm)
#set enum(indent: 2mm, body-indent: 5mm)
#show raw: set text(font: "Source Code Pro", size: 9pt)
#set raw(theme: none)
"#;

const FONTS: &[(&str, &[u8])] = &[
    (
        "SourceSerif4-Regular.ttf",
        include_bytes!("../assets/fonts/SourceSerif4-Regular.ttf"),
    ),
    (
        "SourceSerif4-Italic.ttf",
        include_bytes!("../assets/fonts/SourceSerif4-Italic.ttf"),
    ),
    (
        "SourceSerif4-Semibold.ttf",
        include_bytes!("../assets/fonts/SourceSerif4-Semibold.ttf"),
    ),
    (
        "SourceCodePro-Regular.ttf",
        include_bytes!("../assets/fonts/SourceCodePro-Regular.ttf"),
    ),
    (
        "NotoSans-Regular.ttf",
        include_bytes!("../assets/fonts/NotoSans-Regular.ttf"),
    ),
    (
        "NotoSansHebrew-Regular.ttf",
        include_bytes!("../assets/fonts/NotoSansHebrew-Regular.ttf"),
    ),
    (
        "NotoSansArabic-Regular.ttf",
        include_bytes!("../assets/fonts/NotoSansArabic-Regular.ttf"),
    ),
    (
        "NotoSansDevanagari-Regular.ttf",
        include_bytes!("../assets/fonts/NotoSansDevanagari-Regular.ttf"),
    ),
    (
        "NotoSansJP-Regular.ttf",
        include_bytes!("../assets/fonts/NotoSansJP-Regular.ttf"),
    ),
];

#[derive(Debug, Clone, Copy)]
pub enum Publication<'a> {
    Document {
        markdown: &'a str,
    },
    Work {
        title: &'a str,
        documents: &'a [String],
    },
}

pub trait PdfBackend {
    fn export(&self, publication: Publication<'_>, destination: &Path) -> Result<(), PdfError>;
}

#[derive(Debug, Clone)]
pub struct TypstBackend {
    executable: OsString,
}

impl Default for TypstBackend {
    fn default() -> Self {
        Self {
            executable: env::var_os("CARTA_TYPST").unwrap_or_else(|| OsString::from("typst")),
        }
    }
}

impl TypstBackend {
    pub fn with_executable(executable: impl AsRef<OsStr>) -> Self {
        Self {
            executable: executable.as_ref().to_os_string(),
        }
    }
}

#[derive(Debug)]
pub enum PdfError {
    Io(io::Error),
    BackendUnavailable(PathBuf),
    BackendFailed(String),
}

impl fmt::Display for PdfError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => error.fmt(formatter),
            Self::BackendUnavailable(path) => write!(
                formatter,
                "PDF export requires Typst 0.15.1 or newer; '{}' was not found",
                path.display()
            ),
            Self::BackendFailed(message) => write!(formatter, "Typst PDF export failed: {message}"),
        }
    }
}

impl Error for PdfError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            _ => None,
        }
    }
}

impl From<io::Error> for PdfError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

pub fn export_pdf(publication: Publication<'_>, destination: &Path) -> Result<(), PdfError> {
    TypstBackend::default().export(publication, destination)
}

impl PdfBackend for TypstBackend {
    fn export(&self, publication: Publication<'_>, destination: &Path) -> Result<(), PdfError> {
        let parent = destination
            .parent()
            .filter(|path| !path.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."));
        fs::create_dir_all(parent)?;

        let scratch = tempfile::Builder::new()
            .prefix(".carta-pdf-")
            .tempdir_in(parent)?;
        let fonts = scratch.path().join("fonts");
        fs::create_dir(&fonts)?;
        for (name, bytes) in FONTS {
            fs::write(fonts.join(name), bytes)?;
        }

        let source_path = scratch.path().join("source.typ");
        let output_path = scratch.path().join("output.pdf");
        fs::write(&source_path, publication_source(publication))?;

        let output = Command::new(&self.executable)
            .arg("compile")
            .arg("--root")
            .arg(scratch.path())
            .arg("--font-path")
            .arg(&fonts)
            .arg("--ignore-system-fonts")
            .arg("--ignore-embedded-fonts")
            .arg(&source_path)
            .arg(&output_path)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .output()
            .map_err(|error| {
                if error.kind() == io::ErrorKind::NotFound {
                    PdfError::BackendUnavailable(PathBuf::from(self.executable.clone()))
                } else {
                    PdfError::Io(error)
                }
            })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_owned();
            let message = if stderr.is_empty() {
                format!("process exited with {}", output.status)
            } else {
                stderr
            };
            return Err(PdfError::BackendFailed(message));
        }

        if !output_path.is_file() || fs::metadata(&output_path)?.len() == 0 {
            return Err(PdfError::BackendFailed(
                "renderer completed without producing a PDF".to_owned(),
            ));
        }

        fs::rename(output_path, destination)?;
        Ok(())
    }
}

fn publication_source(publication: Publication<'_>) -> String {
    let mut output = String::with_capacity(CARTA_CLASSIC.len() + 4096);
    output.push_str(CARTA_CLASSIC);
    output.push('\n');

    match publication {
        Publication::Document { markdown } => output.push_str(&markdown_to_typst(markdown)),
        Publication::Work { title, documents } => {
            output.push_str(
                "#align(center + horizon)[#text(size: 20pt, weight: \"semibold\")[",
            );
            push_text(&mut output, title);
            output.push_str("]]\n");
            if !documents.is_empty() {
                output.push_str("#pagebreak()\n\n");
            }
            for (index, markdown) in documents.iter().enumerate() {
                if index != 0 {
                    output.push_str("#pagebreak()\n\n");
                }
                output.push_str(&markdown_to_typst(markdown));
            }
        }
    }

    output.push('\n');
    output
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Frame {
    Paragraph,
    Heading,
    Quote,
    CodeBlock,
    List,
    Item,
    Emphasis,
    Strong,
    Link,
    Image,
    Transparent,
}

fn markdown_to_typst(markdown: &str) -> String {
    let mut output = String::with_capacity(markdown.len().saturating_mul(2));
    let mut stack = Vec::new();

    for event in Parser::new(markdown) {
        match event {
            Event::Start(tag) => {
                let frame = start_tag(&mut output, &tag);
                stack.push(frame);
            }
            Event::End(_) => {
                if let Some(frame) = stack.pop() {
                    end_tag(&mut output, frame);
                }
            }
            Event::Text(text) => {
                if matches!(stack.last(), Some(Frame::CodeBlock)) {
                    output.push_str(&escape_typst_string(&text));
                } else {
                    push_text(&mut output, &text);
                }
            }
            Event::Code(code) => {
                output.push_str("#raw(\"");
                output.push_str(&escape_typst_string(&code));
                output.push_str("\", block: false)");
            }
            Event::Html(html) => push_text(&mut output, &html),
            Event::SoftBreak => push_text(&mut output, " "),
            Event::HardBreak => output.push_str("#linebreak()"),
            Event::Rule => output.push_str("#line(length: 100%)\n\n"),
            _ => {}
        }
    }

    output
}

fn start_tag(output: &mut String, tag: &Tag<'_>) -> Frame {
    match tag {
        Tag::Paragraph => Frame::Paragraph,
        Tag::Heading { level, .. } => {
            output.push_str("#heading(level: ");
            output.push_str(heading_number(*level));
            output.push_str(")[");
            Frame::Heading
        }
        Tag::BlockQuote(_) => {
            output.push_str("#quote[");
            Frame::Quote
        }
        Tag::CodeBlock(_) => {
            output.push_str("#raw(\"");
            Frame::CodeBlock
        }
        Tag::List(start) => {
            match start {
                None => output.push_str("#list(\n"),
                Some(1) => output.push_str("#enum(\n"),
                Some(start) => {
                    output.push_str("#enum(start: ");
                    output.push_str(&start.to_string());
                    output.push_str(",\n");
                }
            }
            Frame::List
        }
        Tag::Item => {
            output.push('[');
            Frame::Item
        }
        Tag::Emphasis => {
            output.push_str("#emph[");
            Frame::Emphasis
        }
        Tag::Strong => {
            output.push_str("#strong[");
            Frame::Strong
        }
        Tag::Link { dest_url, .. } if printable_link(dest_url) => {
            output.push_str("#link(\"");
            output.push_str(&escape_typst_string(dest_url));
            output.push_str("\")[");
            Frame::Link
        }
        Tag::Link { .. } => Frame::Transparent,
        Tag::Image { .. } => {
            output.push_str("#emph[");
            Frame::Image
        }
        _ => Frame::Transparent,
    }
}

fn end_tag(output: &mut String, frame: Frame) {
    match frame {
        Frame::Paragraph => output.push_str("\n\n"),
        Frame::Heading | Frame::Quote => output.push_str("]\n\n"),
        Frame::CodeBlock => output.push_str("\", block: true)\n\n"),
        Frame::List => output.push_str(")\n\n"),
        Frame::Item => output.push_str("],\n"),
        Frame::Emphasis | Frame::Strong | Frame::Link | Frame::Image => output.push(']'),
        Frame::Transparent => {}
    }
}

fn printable_link(destination: &str) -> bool {
    destination.starts_with("https://")
        || destination.starts_with("http://")
        || destination.starts_with("mailto:")
}

fn heading_number(level: HeadingLevel) -> &'static str {
    match level {
        HeadingLevel::H1 => "1",
        HeadingLevel::H2 => "2",
        HeadingLevel::H3 => "3",
        HeadingLevel::H4 => "4",
        HeadingLevel::H5 => "5",
        HeadingLevel::H6 => "6",
    }
}

fn push_text(output: &mut String, text: &str) {
    output.push_str("#text(\"");
    output.push_str(&escape_typst_string(text));
    output.push_str("\")");
}

fn escape_typst_string(value: &str) -> String {
    let mut output = String::with_capacity(value.len());
    for character in value.chars() {
        match character {
            '\\' => output.push_str("\\\\"),
            '"' => output.push_str("\\\""),
            '\n' => output.push_str("\\n"),
            '\r' => output.push_str("\\r"),
            '\t' => output.push_str("\\t"),
            character if character.is_control() => output.push('\u{fffd}'),
            character => output.push(character),
        }
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn markdown_semantics_are_rendered_without_executing_authored_typst() {
        let rendered = markdown_to_typst(
            "# Heading\n\nText #set page(width: 1mm) with **strong**, *emphasis*, \
             [link](https://example.com), and \u{0060}code\u{0060}.\n",
        );
        assert!(rendered.contains("#heading(level: 1)["));
        assert!(rendered.contains("#strong["));
        assert!(rendered.contains("#emph["));
        assert!(rendered.contains("#link(\"https://example.com\")["));
        assert!(rendered.contains("#raw(\"code\", block: false)"));
        assert!(rendered.contains("#text(\"Text #set page(width: 1mm) with "));
        assert!(!rendered.contains("\n#set page(width: 1mm)"));
    }

    #[test]
    fn work_publication_has_cover_and_document_page_breaks() {
        let documents = vec!["First.\n".to_owned(), "Second.\n".to_owned()];
        let rendered = publication_source(Publication::Work {
            title: "A [safe] # title",
            documents: &documents,
        });
        assert!(rendered.contains("#text(\"A [safe] # title\")"));
        assert_eq!(rendered.matches("#pagebreak()").count(), documents.len());
    }

    #[test]
    fn ordered_list_start_is_preserved() {
        let rendered = markdown_to_typst("3. third\n4. fourth\n");
        assert!(rendered.contains("#enum(start: 3,"));
    }

    #[test]
    #[ignore = "requires a Typst 0.15.1+ executable in CARTA_TYPST"]
    fn typst_backend_smoke_renders_real_pdf() {
        let executable = env::var_os("CARTA_TYPST")
            .expect("set CARTA_TYPST to a Typst 0.15.1+ executable");
        let temporary = tempfile::tempdir().unwrap();
        let destination = temporary.path().join("smoke.pdf");
        let markdown = "# PDF smoke\n\nPerché — «Carta». العربية हिन्दी 日本語\n\n> Quote\n\n- one\n- two\n\n\`\`\`\nfn main() {}\n\`\`\`\n";

        TypstBackend::with_executable(executable)
            .export(Publication::Document { markdown }, &destination)
            .unwrap();

        let pdf = fs::read(destination).unwrap();
        assert!(pdf.starts_with(b"%PDF-"));
        assert!(pdf.len() > 1_000);
    }

    #[test]
    fn backend_failure_does_not_replace_existing_destination() {
        let temporary = tempfile::tempdir().unwrap();
        let destination = temporary.path().join("existing.pdf");
        fs::write(&destination, b"existing").unwrap();
        let missing = temporary.path().join("typst-does-not-exist");

        let error = TypstBackend::with_executable(missing.as_os_str())
            .export(
                Publication::Document {
                    markdown: "Hello.\n",
                },
                &destination,
            )
            .unwrap_err();

        assert!(matches!(error, PdfError::BackendUnavailable(_)));
        assert_eq!(fs::read(&destination).unwrap(), b"existing");
    }

    #[test]
    fn carta_links_remain_text_instead_of_becoming_broken_pdf_links() {
        let rendered = markdown_to_typst("[target](carta:doc:1234)");
        assert!(rendered.contains("#text(\"target\")"));
        assert!(!rendered.contains("#link("));
    }
}
