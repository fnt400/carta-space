use std::ops::Range;
use std::str::FromStr;

use carta_format::{DocumentId, Timestamp, WorkId};
use pulldown_cmark::{Event, Parser, Tag, TagEnd};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum CartaLinkTarget {
    Document(DocumentId),
    Work(WorkId),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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

/** External web references, including plain URLs in CommonMark text.
 * Offsets always refer to the unmodified UTF-8 Markdown source.
 */
pub fn extract_web_urls(markdown: &str) -> Vec<MarkdownLink> {
    let mut links: Vec<MarkdownLink> = extract_markdown_links(markdown)
        .into_iter()
        .filter(|link| supported_external_url(link.destination()))
        .collect();
    let mut code_blocks = 0_usize;
    let mut link_depth = 0_usize;
    for (event, source) in Parser::new(markdown).into_offset_iter() {
        match event {
            Event::Start(Tag::CodeBlock(_)) => code_blocks += 1,
            Event::End(TagEnd::CodeBlock) => code_blocks = code_blocks.saturating_sub(1),
            Event::Start(Tag::Link { .. }) => link_depth += 1,
            Event::End(TagEnd::Link) => link_depth = link_depth.saturating_sub(1),
            Event::Text(_) if code_blocks == 0 && link_depth == 0 => {
                let fragment = &markdown[source.clone()];
                let mut from = 0;
                while from < fragment.len() {
                    let next_http = fragment[from..].find("http://");
                    let next_https = fragment[from..].find("https://");
                    let relative = match (next_http, next_https) {
                        (Some(a), Some(b)) => a.min(b),
                        (Some(a), None) | (None, Some(a)) => a,
                        (None, None) => break,
                    };
                    let start = from + relative;
                    let previous = fragment[..start].chars().next_back();
                    if previous.is_some_and(|ch| ch.is_alphanumeric() || ch == '/' || ch == '@') {
                        from = start + 4;
                        continue;
                    }
                    let candidate = &fragment[start..];
                    let length = candidate
                        .find(|ch: char| {
                            ch.is_whitespace()
                                || matches!(
                                    ch,
                                    '<' | '>' | '"' | '\'' | '`' | '[' | ']' | '(' | ')'
                                )
                        })
                        .unwrap_or(candidate.len());
                    let raw = &candidate[..length];
                    let destination = raw.trim_end_matches(|ch: char| {
                        matches!(ch, '.' | ',' | ';' | ':' | '!' | '?')
                    });
                    if supported_external_url(destination) {
                        links.push(MarkdownLink {
                            source_range: (source.start + start)
                                ..(source.start + start + destination.len()),
                            destination: destination.to_owned(),
                            carta_target: None,
                        });
                    }
                    from = start + length.max(1);
                }
            }
            _ => {}
        }
    }
    links.sort_by_key(|link| link.source_range.start);
    links
}

pub fn supported_external_url(destination: &str) -> bool {
    let Some(rest) = destination
        .strip_prefix("https://")
        .or_else(|| destination.strip_prefix("http://"))
    else {
        return false;
    };
    !rest.is_empty()
        && !rest.starts_with('/')
        && !rest.chars().any(|ch| ch.is_control() || ch.is_whitespace())
}

pub fn web_url_at_byte_offset(markdown: &str, byte_offset: usize) -> Option<MarkdownLink> {
    if byte_offset > markdown.len() || !markdown.is_char_boundary(byte_offset) {
        return None;
    }
    extract_web_urls(markdown)
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

#[cfg(test)]
mod web_url_tests {
    use super::*;

    #[test]
    fn finds_plain_and_markdown_web_links_by_utf8_offsets() {
        let text = "è https://example.org/café, [site](https://example.net) fine";
        let first = text.find("https://").unwrap();
        assert_eq!(
            web_url_at_byte_offset(text, first + 6)
                .unwrap()
                .destination(),
            "https://example.org/café"
        );
        let label = text.find("[site]").unwrap();
        assert_eq!(
            web_url_at_byte_offset(text, label).unwrap().destination(),
            "https://example.net"
        );
        assert!(web_url_at_byte_offset(text, text.find("fine").unwrap()).is_none());
    }

    #[test]
    fn does_not_open_code_or_non_web_schemes() {
        let text = "`https://example.org`\n\n```\nhttps://example.net\n```\n\n[javascript](javascript:alert(1))";
        assert!(extract_web_urls(text).is_empty());
        assert!(!supported_external_url("file:///etc/passwd"));
        assert!(!supported_external_url("https://somewhere\nmalicious"));
    }
}
