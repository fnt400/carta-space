//! Incremental presentation-only Markdown syntax styling.
//! Source offsets remain the source of truth: no Markdown is rewritten.
use carta_core::DocumentId;
use pulldown_cmark::{Event, Parser, Tag, TagEnd};
use std::collections::HashMap;

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

/// Converts overlapping CommonMark ranges into non-overlapping color runs.
/// This is computed once per changed Document, not once per viewport redraw.
/// Visible styling then performs only logarithmic lookups.
fn resolve(ranges: Vec<Range>) -> Vec<Range> {
    let mut events = Vec::with_capacity(ranges.len() * 2);
    for range in ranges {
        if range.start < range.end {
            events.push((range.start, range.syntax as usize, true));
            events.push((range.end, range.syntax as usize, false));
        }
    }
    events.sort_unstable_by_key(|(byte, _, _)| *byte);
    let mut active = [0_usize; 6];
    let mut last = 0;
    let mut result: Vec<Range> = Vec::new();
    let mut index = 0;
    const PRIORITY: [Syntax; 6] = [
        Syntax::Code, Syntax::Link, Syntax::Strong,
        Syntax::Emphasis, Syntax::Heading, Syntax::Quote,
    ];
    while index < events.len() {
        let point = events[index].0;
        if last < point {
            if let Some(syntax) = PRIORITY.into_iter()
                .find(|syntax| active[*syntax as usize] > 0)
            {
                if let Some(previous) = result.last_mut() {
                    if previous.end == last && previous.syntax == syntax {
                        previous.end = point;
                    } else {
                        result.push(Range { start: last, end: point, syntax });
                    }
                } else {
                    result.push(Range { start: last, end: point, syntax });
                }
            }
        }
        while index < events.len() && events[index].0 == point {
            let (_, category, entering) = events[index];
            if entering {
                active[category] += 1;
            } else {
                active[category] = active[category].saturating_sub(1);
            }
            index += 1;
        }
        last = point;
    }
    result
}

/// `ranges` must be the non-overlapping result stored in `Cache`.
pub fn style_at(ranges: &[Range], byte: usize) -> Option<Syntax> {
    let next = ranges.partition_point(|range| range.end <= byte);
    ranges.get(next)
        .filter(|range| range.start <= byte)
        .map(|range| range.syntax)
}

#[derive(Default)]
pub struct Cache {
    documents: HashMap<DocumentId, (u64, Vec<Range>)>,
}

impl Cache {
    /// Generations are incremented by the viewport layout only for changed
    /// Documents. Normal navigation never hashes or reparses whole Documents.
    pub fn ranges(&mut self, id: DocumentId, text: &str, generation: u64) -> &[Range] {
        let entry = self.documents.entry(id).or_insert_with(|| (generation, resolve(parse(text))));
        if entry.0 != generation {
            *entry = (generation, resolve(parse(text)));
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
    fn resolved_styles_have_nonoverlapping_runs_with_logarithmic_lookup() {
        let text = "# **strong** and *soft* in a heading\n";
        let resolved = resolve(parse(text));
        assert!(resolved.windows(2).all(|pair| pair[0].end <= pair[1].start));
        let strong = text.find("strong").unwrap();
        assert_eq!(style_at(&resolved, strong), Some(Syntax::Strong));
        let soft = text.find("soft").unwrap();
        assert_eq!(style_at(&resolved, soft), Some(Syntax::Emphasis));
        assert_eq!(style_at(&resolved, 0), Some(Syntax::Heading));
    }

    #[test]
    fn nested_styles_prioritize_specific_tokens() {
        let text = "# a **strong** title\n";
        let r = parse(text);
        let n = text.find("strong").unwrap();
        assert_eq!(syntax_at(&r, n, n + 1), Some(Syntax::Strong));
    }
}
