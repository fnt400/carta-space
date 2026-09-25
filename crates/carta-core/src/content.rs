use std::ops::Range;
use std::str::FromStr;

use carta_format::{DocumentId, Timestamp, WorkId};
use pulldown_cmark::{Event, Parser, Tag, TagEnd};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CartaLinkTarget {
    Document(DocumentId),
    Work(WorkId),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MarkdownLink {
    source_range: Range<usize>,
    destination: String,
    carta_target: Option<CartaLinkTarget>,
}

impl MarkdownLink {
    pub fn source_range(&self) -> Range<usize> {
        self.source_range.clone()
    }

    pub fn destination(&self) -> &str {
        &self.destination
    }

    pub fn carta_target(&self) -> Option<CartaLinkTarget> {
        self.carta_target
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LinkResolution {
    Document(DocumentId),
    Work(WorkId),
    Unresolved(CartaLinkTarget),
}

pub fn extract_markdown_links(markdown: &str) -> Vec<MarkdownLink> {
    let mut links = Vec::new();
    let mut active = Vec::new();

    for (event, range) in Parser::new(markdown).into_offset_iter() {
        match event {
            Event::Start(Tag::Link { dest_url, .. }) => {
                active.push((range.start, dest_url.into_string()));
            }
            Event::End(TagEnd::Link) => {
                if let Some((start, destination)) = active.pop() {
                    links.push(MarkdownLink {
                        source_range: start..range.end,
                        carta_target: parse_carta_target(&destination),
                        destination,
                    });
                }
            }
            _ => {}
        }
    }

    links.sort_by_key(|link| link.source_range.start);
    links
}

pub fn link_at_byte_offset(markdown: &str, byte_offset: usize) -> Option<MarkdownLink> {
    if byte_offset > markdown.len() || !markdown.is_char_boundary(byte_offset) {
        return None;
    }
    extract_markdown_links(markdown)
        .into_iter()
        .filter(|link| {
            link.source_range.contains(&byte_offset)
                || (byte_offset == markdown.len() && link.source_range.end == byte_offset)
        })
        .min_by_key(|link| link.source_range.end - link.source_range.start)
}

pub(crate) fn document_label(content: &str, created: Timestamp) -> String {
    let Some((first_start, first_line)) = content
        .split_inclusive('\n')
        .scan(0, |offset, line| {
            let start = *offset;
            *offset += line.len();
            Some((start, line.strip_suffix('\n').unwrap_or(line)))
        })
        .find(|(_, line)| !line.trim().is_empty())
    else {
        return created.to_string();
    };

    let mut heading = None;
    let mut text = String::new();
    for (event, range) in Parser::new(content).into_offset_iter() {
        match event {
            Event::Start(Tag::Heading { .. }) if range.start == first_start => heading = Some(()),
            Event::Text(value) | Event::Code(value) if heading.is_some() => text.push_str(&value),
            Event::SoftBreak | Event::HardBreak if heading.is_some() => text.push(' '),
            Event::End(TagEnd::Heading(_)) if heading.is_some() => return text,
            _ => {}
        }
    }

    first_line.to_owned()
}

fn parse_carta_target(destination: &str) -> Option<CartaLinkTarget> {
    if let Some(id) = destination.strip_prefix("carta:doc:") {
        DocumentId::from_str(id).ok().map(CartaLinkTarget::Document)
    } else if let Some(id) = destination.strip_prefix("carta:work:") {
        WorkId::from_str(id).ok().map(CartaLinkTarget::Work)
    } else {
        None
    }
}
