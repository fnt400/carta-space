use std::ops::Range;

use carta_format::{DocumentId, Timestamp};
use unicode_casefold::UnicodeCaseFold;
use unicode_normalization::{char::is_combining_mark, UnicodeNormalization};

use crate::MarkdownLink;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Backlink {
    source: DocumentId,
    link: MarkdownLink,
}

impl Backlink {
    pub(crate) fn new(source: DocumentId, link: MarkdownLink) -> Self {
        Self { source, link }
    }

    pub fn source(&self) -> DocumentId {
        self.source
    }

    pub fn link(&self) -> &MarkdownLink {
        &self.link
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchResult {
    document: DocumentId,
    created: Timestamp,
    label: String,
    occurrence: Range<usize>,
    context: String,
    context_range: Range<usize>,
}

impl SearchResult {
    pub(crate) fn new(
        document: DocumentId,
        created: Timestamp,
        label: String,
        occurrence: Range<usize>,
        content: &str,
    ) -> Self {
        let context_start = content[..occurrence.start]
            .rfind('\n')
            .map_or(0, |index| index + 1);
        let context_end = content[occurrence.end..]
            .find('\n')
            .map_or(content.len(), |index| occurrence.end + index);
        Self {
            document,
            created,
            label,
            occurrence,
            context: content[context_start..context_end].to_owned(),
            context_range: context_start..context_end,
        }
    }

    pub fn document(&self) -> DocumentId {
        self.document
    }

    pub fn created(&self) -> Timestamp {
        self.created
    }

    pub fn label(&self) -> &str {
        &self.label
    }

    pub fn occurrence(&self) -> Range<usize> {
        self.occurrence.clone()
    }

    pub fn context(&self) -> &str {
        &self.context
    }

    pub fn context_range(&self) -> Range<usize> {
        self.context_range.clone()
    }
}

#[derive(Debug, Clone, Copy)]
pub struct DocumentTextRegion<'a> {
    document: DocumentId,
    text: &'a str,
}

impl<'a> DocumentTextRegion<'a> {
    pub fn new(document: DocumentId, text: &'a str) -> Self {
        Self { document, text }
    }

    pub fn document(self) -> DocumentId {
        self.document
    }

    pub fn text(self) -> &'a str {
        self.text
    }
}

pub trait LeapTextRegion {
    fn document(&self) -> DocumentId;
    fn text(&self) -> &str;
}

impl LeapTextRegion for DocumentTextRegion<'_> {
    fn document(&self) -> DocumentId {
        self.document
    }

    fn text(&self) -> &str {
        self.text
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LeapDirection {
    Forward,
    Backward,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LeapPosition {
    region: usize,
    byte_offset: usize,
}

impl LeapPosition {
    pub fn new(region: usize, byte_offset: usize) -> Self {
        Self {
            region,
            byte_offset,
        }
    }

    pub fn region(self) -> usize {
        self.region
    }

    pub fn byte_offset(self) -> usize {
        self.byte_offset
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LeapMatch {
    document: DocumentId,
    position: LeapPosition,
    range: Range<usize>,
    wrapped: bool,
}

impl LeapMatch {
    pub fn document(&self) -> DocumentId {
        self.document
    }

    pub fn position(&self) -> LeapPosition {
        self.position
    }

    pub fn range(&self) -> Range<usize> {
        self.range.clone()
    }

    pub fn wrapped(&self) -> bool {
        self.wrapped
    }
}

#[derive(Debug, Clone)]
pub struct LeapSession {
    direction: LeapDirection,
    origin: LeapPosition,
    query: String,
    current: Option<LeapMatch>,
}

impl LeapSession {
    pub fn new(direction: LeapDirection, origin: LeapPosition) -> Self {
        Self {
            direction,
            origin,
            query: String::new(),
            current: None,
        }
    }

    pub fn with_query<R: LeapTextRegion>(
        direction: LeapDirection,
        origin: LeapPosition,
        query: impl Into<String>,
        regions: &[R],
    ) -> Self {
        let mut session = Self::new(direction, origin);
        session.query = query.into();
        session.update(regions);
        session
    }

    pub fn push_str<R: LeapTextRegion>(&mut self, value: &str, regions: &[R]) {
        self.query.push_str(value);
        self.update(regions);
    }

    pub fn backspace<R: LeapTextRegion>(&mut self, regions: &[R]) -> bool {
        let Some((index, _)) = self.query.char_indices().next_back() else {
            return false;
        };
        self.query.truncate(index);
        self.update(regions);
        true
    }

    pub fn query(&self) -> &str {
        &self.query
    }

    pub fn direction(&self) -> LeapDirection {
        self.direction
    }

    pub fn origin(&self) -> LeapPosition {
        self.origin
    }

    pub fn current_match(&self) -> Option<&LeapMatch> {
        self.current.as_ref()
    }

    pub fn cursor(&self) -> LeapPosition {
        self.current
            .as_ref()
            .map_or(self.origin, LeapMatch::position)
    }

    pub fn repeat<R: LeapTextRegion>(&mut self, regions: &[R]) -> bool {
        if self.query.is_empty() {
            return false;
        }
        let Some(current) = self.current.as_ref() else {
            return false;
        };
        let origin = current.position();
        let Some(next) = leap_match(regions, origin, &self.query, self.direction, false) else {
            return false;
        };
        self.current = Some(next);
        true
    }

    fn update<R: LeapTextRegion>(&mut self, regions: &[R]) {
        self.current = if self.query.is_empty() {
            None
        } else {
            leap_match(regions, self.origin, &self.query, self.direction, true)
        };
    }
}

/// Disposable session state for Leap Again; it is never archive metadata.
#[derive(Debug, Clone, Default)]
pub struct LeapRuntime {
    remembered_query: Option<String>,
}

impl LeapRuntime {
    pub fn remember(&mut self, session: &LeapSession) {
        if !session.query.is_empty() {
            self.remembered_query = Some(session.query.clone());
        }
    }

    pub fn remembered_query(&self) -> Option<&str> {
        self.remembered_query.as_deref()
    }

    pub fn leap_again<R: LeapTextRegion>(
        &self,
        direction: LeapDirection,
        origin: LeapPosition,
        regions: &[R],
    ) -> Option<LeapMatch> {
        leap_match(
            regions,
            origin,
            self.remembered_query.as_deref()?,
            direction,
            false,
        )
    }
}

pub(crate) fn literal_match(text: &str, query: &str) -> Option<Range<usize>> {
    let query = folded(query);
    if query.is_empty() {
        return None;
    }
    first_folded_match(text, &query)
}

fn leap_match<R: LeapTextRegion>(
    regions: &[R],
    origin: LeapPosition,
    query: &str,
    direction: LeapDirection,
    include_origin: bool,
) -> Option<LeapMatch> {
    if query.is_empty() || origin.region >= regions.len() {
        return None;
    }
    let origin_text = regions[origin.region].text();
    if origin.byte_offset > origin_text.len() || !origin_text.is_char_boundary(origin.byte_offset) {
        return None;
    }

    let pattern = compile_leap_pattern(query);
    if pattern.is_empty() {
        return None;
    }

    match direction {
        LeapDirection::Forward => {
            for (index, region) in regions.iter().enumerate().skip(origin.region) {
                let minimum = if index == origin.region {
                    origin.byte_offset
                } else {
                    0
                };
                if let Some(range) = leap_ranges_forward(region.text(), &pattern).find(|range| {
                    range.start >= minimum
                        && (include_origin
                            || index != origin.region
                            || range.start != origin.byte_offset)
                }) {
                    return Some(make_leap_match(region.document(), index, range, false));
                }
            }
            for (index, region) in regions.iter().enumerate().take(origin.region + 1) {
                if let Some(range) = leap_ranges_forward(region.text(), &pattern).find(|range| {
                    index < origin.region
                        || range.start < origin.byte_offset
                        || (!include_origin && range.start == origin.byte_offset)
                }) {
                    return Some(make_leap_match(region.document(), index, range, true));
                }
            }
        }
        LeapDirection::Backward => {
            for index in (0..=origin.region).rev() {
                let maximum = if index == origin.region {
                    origin.byte_offset
                } else {
                    regions[index].text().len()
                };
                if let Some(range) = leap_ranges_backward(regions[index].text(), &pattern)
                    .find(|range| range.start < maximum)
                {
                    return Some(make_leap_match(
                        regions[index].document(),
                        index,
                        range,
                        false,
                    ));
                }
            }
            for index in (origin.region..regions.len()).rev() {
                if let Some(range) = leap_ranges_backward(regions[index].text(), &pattern)
                    .find(|range| index > origin.region || range.start >= origin.byte_offset)
                {
                    return Some(make_leap_match(
                        regions[index].document(),
                        index,
                        range,
                        true,
                    ));
                }
            }
        }
    }
    None
}

fn make_leap_match(
    document: DocumentId,
    index: usize,
    range: Range<usize>,
    wrapped: bool,
) -> LeapMatch {
    LeapMatch {
        document,
        position: LeapPosition::new(index, range.start),
        range,
        wrapped,
    }
}

#[derive(Debug, Clone)]
struct LeapPatternChar {
    base: char,
    marks: String,
    folded_base: String,
    uppercase: bool,
}

fn compile_leap_pattern(query: &str) -> Vec<LeapPatternChar> {
    query
        .chars()
        .map(|character| {
            let (base, marks) = decomposed_pattern_char(character);
            let uppercase = character.is_uppercase();
            LeapPatternChar {
                base,
                marks,
                folded_base: if uppercase {
                    String::new()
                } else {
                    folded_char(base)
                },
                uppercase,
            }
        })
        .collect()
}

fn leap_ranges_forward<'a>(
    text: &'a str,
    pattern: &'a [LeapPatternChar],
) -> impl Iterator<Item = Range<usize>> + 'a {
    text.char_indices().filter_map(move |(start, _)| {
        match_pattern_at(text, start, pattern).map(|end| start..end)
    })
}

fn leap_ranges_backward<'a>(
    text: &'a str,
    pattern: &'a [LeapPatternChar],
) -> impl Iterator<Item = Range<usize>> + 'a {
    text.char_indices().rev().filter_map(move |(start, _)| {
        match_pattern_at(text, start, pattern).map(|end| start..end)
    })
}

fn match_pattern_at(text: &str, start: usize, pattern: &[LeapPatternChar]) -> Option<usize> {
    let mut text_chars = text[start..].char_indices();
    let mut end = start;
    for pattern_char in pattern {
        let (relative, text_char) = text_chars.next()?;
        if !cat_leap_char_matches(pattern_char, text_char) {
            return None;
        }
        end = start + relative + text_char.len_utf8();
    }
    Some(end)
}

fn cat_leap_char_matches(pattern: &LeapPatternChar, text: char) -> bool {
    let text_base = decomposed_base(text);
    if !pattern.marks.is_empty() && !marks_match(&pattern.marks, text) {
        return false;
    }

    if pattern.uppercase {
        pattern.base == text_base && text.is_uppercase()
    } else {
        folded_char_matches(&pattern.folded_base, text_base)
    }
}

fn decomposed_pattern_char(character: char) -> (char, String) {
    let mut encoded = [0_u8; 4];
    let value: &str = character.encode_utf8(&mut encoded);
    let mut base = None;
    let mut marks = String::new();
    for decomposed in value.nfd() {
        if is_combining_mark(decomposed) {
            marks.push(decomposed);
        } else if base.is_none() {
            base = Some(decomposed);
        }
    }
    (base.unwrap_or(character), marks)
}

fn decomposed_base(character: char) -> char {
    let mut encoded = [0_u8; 4];
    let value: &str = character.encode_utf8(&mut encoded);
    value
        .nfd()
        .find(|decomposed| !is_combining_mark(*decomposed))
        .unwrap_or(character)
}

fn marks_match(expected: &str, character: char) -> bool {
    let mut encoded = [0_u8; 4];
    let value: &str = character.encode_utf8(&mut encoded);
    expected
        .chars()
        .eq(value.nfd().filter(|decomposed| is_combining_mark(*decomposed)))
}

fn folded_char(character: char) -> String {
    let mut encoded = [0_u8; 4];
    let value: &str = character.encode_utf8(&mut encoded);
    value.case_fold().collect()
}

fn folded_char_matches(expected: &str, character: char) -> bool {
    let mut encoded = [0_u8; 4];
    let value: &str = character.encode_utf8(&mut encoded);
    expected.chars().eq(value.case_fold())
}

#[cfg(test)]
fn leap_matches(text: &str, query: &str) -> Vec<Range<usize>> {
    let pattern = compile_leap_pattern(query);
    if pattern.is_empty() {
        return Vec::new();
    }
    leap_ranges_forward(text, &pattern).collect()
}

fn first_folded_match(text: &str, folded_query: &str) -> Option<Range<usize>> {
    let (folded_text, boundaries) = folded_with_boundaries(text);
    let mut search_from = 0;
    while search_from <= folded_text.len() {
        let relative = folded_text[search_from..].find(folded_query)?;
        let start = search_from + relative;
        let end = start + folded_query.len();
        if let (Some(original_start), Some(original_end)) = (
            boundary_at(&boundaries, start),
            boundary_at(&boundaries, end),
        ) {
            return Some(original_start..original_end);
        }
        let advance = folded_text[start..]
            .chars()
            .next()
            .map_or(1, char::len_utf8);
        search_from = start + advance;
    }
    None
}

fn boundary_at(boundaries: &[(usize, usize)], offset: usize) -> Option<usize> {
    boundaries
        .binary_search_by_key(&offset, |(folded, _)| *folded)
        .ok()
        .map(|index| boundaries[index].1)
}

fn folded(value: &str) -> String {
    value.case_fold().collect()
}

fn folded_with_boundaries(value: &str) -> (String, Vec<(usize, usize)>) {
    let mut output = String::new();
    let mut boundaries = vec![(0, 0)];
    for (start, character) in value.char_indices() {
        let end = start + character.len_utf8();
        let mut encoded = [0_u8; 4];
        let value: &str = character.encode_utf8(&mut encoded);
        output.extend(value.case_fold());
        if let Some((folded, original)) = boundaries.last_mut() {
            if *folded == output.len() {
                *original = end;
                continue;
            }
        }
        boundaries.push((output.len(), end));
    }
    (output, boundaries)
}

#[cfg(test)]
mod cat_leap_tests {
    use super::*;

    #[test]
    fn active_leap_repeat_advances_without_changing_the_original_origin() {
        let document = DocumentId::new_v7();
        let regions = [DocumentTextRegion::new(document, "x x x")];
        let origin = LeapPosition::new(0, 0);
        let mut session = LeapSession::with_query(LeapDirection::Forward, origin, "x", &regions);

        assert_eq!(session.origin(), origin);
        assert_eq!(session.cursor(), LeapPosition::new(0, 0));

        assert!(session.repeat(&regions));
        assert_eq!(session.origin(), origin);
        assert_eq!(session.cursor(), LeapPosition::new(0, 2));

        assert!(session.repeat(&regions));
        assert_eq!(session.origin(), origin);
        assert_eq!(session.cursor(), LeapPosition::new(0, 4));
    }

    #[test]
    fn lowercase_leap_pattern_matches_both_cases_but_uppercase_is_strict() {
        assert_eq!(
            leap_matches("me Me mE ME", "me"),
            vec![0..2, 3..5, 6..8, 9..11]
        );
        assert_eq!(leap_matches("a A á Á", "A"), vec![2..3, 7..9]);
    }

    #[test]
    fn plain_leap_character_matches_accented_text() {
        assert_eq!(leap_matches("a á A Á", "a"), vec![0..1, 2..4, 5..6, 7..9]);
        assert_eq!(leap_matches("a á A Á", "á"), vec![2..4, 7..9]);
    }

    #[test]
    fn leap_scanner_preserves_wrap_and_backward_search() {
        let first = DocumentId::new_v7();
        let second = DocumentId::new_v7();
        let regions = [
            DocumentTextRegion::new(first, "alpha café"),
            DocumentTextRegion::new(second, "beta CAFÉ gamma"),
        ];

        let forward = leap_match(
            &regions,
            LeapPosition::new(0, "alpha ".len()),
            "cafe",
            LeapDirection::Forward,
            true,
        )
        .unwrap();
        assert_eq!(forward.document(), first);
        assert_eq!(forward.range(), 6..11);
        assert!(!forward.wrapped());

        let backward = leap_match(
            &regions,
            LeapPosition::new(0, 0),
            "beta",
            LeapDirection::Backward,
            true,
        )
        .unwrap();
        assert_eq!(backward.document(), second);
        assert!(backward.wrapped());
    }

    #[test]
    fn leap_no_match_handles_a_large_region_without_changing_semantics() {
        let document = DocumentId::new_v7();
        let text = "x".repeat(100_000);
        let regions = [DocumentTextRegion::new(document, &text)];

        assert!(leap_match(
            &regions,
            LeapPosition::new(0, 0),
            "needle",
            LeapDirection::Forward,
            true,
        )
        .is_none());
        assert!(leap_match(
            &regions,
            LeapPosition::new(0, text.len()),
            "needle",
            LeapDirection::Backward,
            true,
        )
        .is_none());
    }
}
