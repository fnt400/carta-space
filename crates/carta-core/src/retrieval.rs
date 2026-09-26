use std::collections::BTreeMap;
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

    pub fn with_query(
        direction: LeapDirection,
        origin: LeapPosition,
        query: impl Into<String>,
        regions: &[DocumentTextRegion<'_>],
    ) -> Self {
        let mut session = Self::new(direction, origin);
        session.query = query.into();
        session.update(regions);
        session
    }

    pub fn push_str(&mut self, value: &str, regions: &[DocumentTextRegion<'_>]) {
        self.query.push_str(value);
        self.update(regions);
    }

    pub fn backspace(&mut self, regions: &[DocumentTextRegion<'_>]) -> bool {
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

    pub fn repeat(&mut self, regions: &[DocumentTextRegion<'_>]) -> bool {
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

    fn update(&mut self, regions: &[DocumentTextRegion<'_>]) {
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

    pub fn leap_again(
        &self,
        direction: LeapDirection,
        origin: LeapPosition,
        regions: &[DocumentTextRegion<'_>],
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
    all_matches(text, &query).into_iter().next()
}

fn leap_match(
    regions: &[DocumentTextRegion<'_>],
    origin: LeapPosition,
    query: &str,
    direction: LeapDirection,
    include_origin: bool,
) -> Option<LeapMatch> {
    if query.is_empty() || origin.region >= regions.len() {
        return None;
    }
    let origin_text = regions[origin.region].text;
    if origin.byte_offset > origin_text.len() || !origin_text.is_char_boundary(origin.byte_offset) {
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
                if let Some(range) = leap_matches(region.text, query).into_iter().find(|range| {
                    range.start >= minimum
                        && (include_origin
                            || index != origin.region
                            || range.start != origin.byte_offset)
                }) {
                    return Some(make_leap_match(*region, index, range, false));
                }
            }
            for (index, region) in regions.iter().enumerate().take(origin.region + 1) {
                if let Some(range) = leap_matches(region.text, query).into_iter().find(|range| {
                    index < origin.region
                        || range.start < origin.byte_offset
                        || (!include_origin && range.start == origin.byte_offset)
                }) {
                    return Some(make_leap_match(*region, index, range, true));
                }
            }
        }
        LeapDirection::Backward => {
            for index in (0..=origin.region).rev() {
                let maximum = if index == origin.region {
                    origin.byte_offset
                } else {
                    regions[index].text.len()
                };
                if let Some(range) = leap_matches(regions[index].text, query)
                    .into_iter()
                    .rev()
                    .find(|range| range.start < maximum)
                {
                    return Some(make_leap_match(regions[index], index, range, false));
                }
            }
            for index in (origin.region..regions.len()).rev() {
                if let Some(range) = leap_matches(regions[index].text, query)
                    .into_iter()
                    .rev()
                    .find(|range| index > origin.region || range.start >= origin.byte_offset)
                {
                    return Some(make_leap_match(regions[index], index, range, true));
                }
            }
        }
    }
    None
}

fn make_leap_match(
    region: DocumentTextRegion<'_>,
    index: usize,
    range: Range<usize>,
    wrapped: bool,
) -> LeapMatch {
    LeapMatch {
        document: region.document,
        position: LeapPosition::new(index, range.start),
        range,
        wrapped,
    }
}

fn leap_matches(text: &str, query: &str) -> Vec<Range<usize>> {
    let pattern: Vec<char> = query.chars().collect();
    if pattern.is_empty() {
        return Vec::new();
    }

    let starts: Vec<(usize, char)> = text.char_indices().collect();
    let mut matches = Vec::new();
    for start_index in 0..starts.len() {
        if start_index + pattern.len() > starts.len() {
            break;
        }
        if pattern
            .iter()
            .zip(starts[start_index..].iter().map(|(_, character)| character))
            .all(|(pattern, text)| cat_leap_char_matches(*pattern, *text))
        {
            let start = starts[start_index].0;
            let end_index = start_index + pattern.len();
            let end = starts.get(end_index).map_or(text.len(), |(byte, _)| *byte);
            matches.push(start..end);
        }
    }
    matches
}

fn cat_leap_char_matches(pattern: char, text: char) -> bool {
    let (pattern_base, pattern_marks) = decomposed_char(pattern);
    let (text_base, text_marks) = decomposed_char(text);

    if !pattern_marks.is_empty() && pattern_marks != text_marks {
        return false;
    }

    if pattern.is_uppercase() {
        pattern_base == text_base && text.is_uppercase()
    } else {
        folded(&pattern_base.to_string()) == folded(&text_base.to_string())
    }
}

fn decomposed_char(character: char) -> (char, String) {
    let decomposed: Vec<char> = character.to_string().nfd().collect();
    let base = decomposed
        .iter()
        .copied()
        .find(|character| !is_combining_mark(*character))
        .unwrap_or(character);
    let marks = decomposed
        .into_iter()
        .filter(|character| is_combining_mark(*character))
        .collect();
    (base, marks)
}

fn all_matches(text: &str, folded_query: &str) -> Vec<Range<usize>> {
    let (folded_text, boundaries) = folded_with_boundaries(text);
    let mut matches = Vec::new();
    let mut search_from = 0;
    while search_from <= folded_text.len() {
        let Some(relative) = folded_text[search_from..].find(folded_query) else {
            break;
        };
        let start = search_from + relative;
        let end = start + folded_query.len();
        if let (Some(original_start), Some(original_end)) =
            (boundaries.get(&start), boundaries.get(&end))
        {
            matches.push(*original_start..*original_end);
        }
        let advance = folded_text[start..]
            .chars()
            .next()
            .map_or(1, char::len_utf8);
        search_from = start + advance;
    }
    matches
}

fn folded(value: &str) -> String {
    value.case_fold().collect()
}

fn folded_with_boundaries(value: &str) -> (String, BTreeMap<usize, usize>) {
    let mut output = String::new();
    let mut boundaries = BTreeMap::new();
    boundaries.insert(0, 0);
    for (start, character) in value.char_indices() {
        let end = start + character.len_utf8();
        let folded_character: String = character.to_string().case_fold().collect();
        output.push_str(&folded_character);
        boundaries.insert(output.len(), end);
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
}
