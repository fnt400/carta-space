//! Incremental presentation-only Markdown syntax styling.
//! Source offsets remain the source of truth: no Markdown is rewritten.
use carta_core::DocumentId;
use pulldown_cmark::{Event, Parser, Tag, TagEnd};
use std::collections::HashMap;
use std::hash::{Hash, Hasher};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Syntax {
    Quote,
    Heading,
    Emphasis,
    Strong,
    Link,
    Code,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Range {
    pub start: usize,
    pub end: usize,
    pub syntax: Syntax,
}

fn parse(text: &str) -> Vec<Range> {
    let mut active = Vec::new();
    let mut ranges = Vec::new();
    for (event, source) in Parser::new(text).into_offset_iter() {
        match event {
            Event::Start(tag) => {
                if let Some(syntax) = start_syntax(&tag) {
                    active.push((syntax, source.start));
                }
            }
            Event::End(tag) => {
                if let Some(syntax) = end_syntax(tag) {
                    if let Some(index) = active.iter().rposition(|(kind, _)| *kind == syntax) {
                        let (_, start) = active.remove(index);
                        ranges.push(Range {
                            start,
                            end: source.end,
                            syntax,
                        });
                    }
                }
            }
            Event::Code(_) => ranges.push(Range {
                start: source.start,
                end: source.end,
                syntax: Syntax::Code,
            }),
            _ => {}
        }
    }
    ranges
}

fn start_syntax(tag: &Tag<'_>) -> Option<Syntax> {
    match tag {
        Tag::BlockQuote(_) => Some(Syntax::Quote),
        Tag::Heading { .. } => Some(Syntax::Heading),
        Tag::Emphasis => Some(Syntax::Emphasis),
        Tag::Strong => Some(Syntax::Strong),
        Tag::Link { .. } | Tag::Image { .. } => Some(Syntax::Link),
        Tag::CodeBlock(_) => Some(Syntax::Code),
        _ => None,
    }
}

fn end_syntax(tag: TagEnd) -> Option<Syntax> {
    match tag {
        TagEnd::BlockQuote(_) => Some(Syntax::Quote),
        TagEnd::Heading(_) => Some(Syntax::Heading),
        TagEnd::Emphasis => Some(Syntax::Emphasis),
        TagEnd::Strong => Some(Syntax::Strong),
        TagEnd::Link | TagEnd::Image => Some(Syntax::Link),
        TagEnd::CodeBlock => Some(Syntax::Code),
        _ => None,
    }
}

#[derive(Default)]
pub struct Cache {
    documents: HashMap<DocumentId, (u64, Vec<Range>)>,
}

impl Cache {
    /// Hashing the contents is linear but cheap. Parsing happens only when
    /// the Document changes; idle redraws never repeat the Markdown parse.
    pub fn ranges(&mut self, id: DocumentId, text: &str) -> &[Range] {
        let mut hash = std::collections::hash_map::DefaultHasher::new();
        text.hash(&mut hash);
        let fingerprint = hash.finish();
        let entry = self
            .documents
            .entry(id)
            .or_insert_with(|| (fingerprint, parse(text)));
        if entry.0 != fingerprint {
            *entry = (fingerprint, parse(text));
        }
        &entry.1
    }
}

pub fn syntax_at(ranges: &[Range], start: usize, end: usize) -> Option<Syntax> {
    // Most specific syntax takes priority over a containing block.
    const PRIORITY: [Syntax; 6] = [
        Syntax::Code,
        Syntax::Link,
        Syntax::Strong,
        Syntax::Emphasis,
        Syntax::Heading,
        Syntax::Quote,
    ];
    PRIORITY.into_iter().find(|kind| {
        ranges
            .iter()
            .any(|range| range.syntax == *kind && range.start < end && start < range.end)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn markdown_styles_include_delimiters_and_preserve_source_offsets() {
        let source =
            "# Title\nA **bold** and *soft* [link](https://example.org) with `code`.\n> quote\n";
        let ranges = parse(source);
        for kind in [
            Syntax::Heading,
            Syntax::Strong,
            Syntax::Emphasis,
            Syntax::Link,
            Syntax::Code,
            Syntax::Quote,
        ] {
            assert!(ranges.iter().any(|r| r.syntax == kind), "missing {kind:?}");
        }
        let bold = source.find("**bold**").unwrap();
        assert_eq!(syntax_at(&ranges, bold, bold + 1), Some(Syntax::Strong));
    }

    #[test]
    fn nested_styles_prioritize_specific_tokens() {
        let text = "# a **strong** title\n";
        let r = parse(text);
        let n = text.find("strong").unwrap();
        assert_eq!(syntax_at(&r, n, n + 1), Some(Syntax::Strong));
    }
}
