//! Optional on-demand Hunspell service; only reports candidate source spans.
use std::io::Write;
use std::ops::Range;
use std::path::Path;
use std::process::{Command, Stdio};

use crate::dictionary_manager::DictionaryLocation;
use carta_core::DocumentId;
use pulldown_cmark::{Event, Parser, Tag, TagEnd};
use unicode_normalization::char::is_combining_mark;

#[derive(Debug, Clone)]
pub struct SpellIssue {
    pub document: DocumentId,
    pub region: usize,
    pub language: String,
    pub start: usize,
    pub end: usize,
    pub word: String,
    pub suggestions: Vec<String>,
}

pub fn valid_language(language: &str) -> bool {
    let bytes = language.as_bytes();
    bytes.len() == 5
        && bytes[..2].iter().all(u8::is_ascii_lowercase)
        && bytes[2] == b'_'
        && bytes[3..].iter().all(u8::is_ascii_uppercase)
}

pub fn system_language() -> String {
    for key in ["LC_ALL", "LC_MESSAGES", "LANG"] {
        if let Ok(locale) = std::env::var(key) {
            let language = locale
                .split('.')
                .next()
                .unwrap_or("")
                .split('@')
                .next()
                .unwrap_or("");
            if valid_language(language) {
                return language.to_owned();
            }
        }
    }
    "en_US".to_owned()
}

/// Only CommonMark text whose raw source span matches the rendered event.
/// This intentionally skips code, HTML, URL destinations and escaped entities.
fn words(source: &str) -> Vec<(Range<usize>, String)> {
    let mut results = Vec::new();
    let mut in_code_block = false;
    for (event, range) in Parser::new(source).into_offset_iter() {
        let rendered = match event {
            Event::Start(Tag::CodeBlock(_)) => {
                in_code_block = true;
                continue;
            }
            Event::End(TagEnd::CodeBlock) => {
                in_code_block = false;
                continue;
            }
            Event::Text(rendered) if !in_code_block => rendered,
            _ => continue,
        };
        let Some(raw) = source.get(range.clone()) else {
            continue;
        };
        if raw != rendered.as_ref() {
            continue;
        }
        let mut begin = None;
        for (index, ch) in raw.char_indices() {
            // Keep elisions (l'éléphant), Italian truncations (po') and
            // decomposed accents in the same source range as their word.
            let apostrophe = matches!(ch, '\'' | '’') && begin.is_some();
            let combining = is_combining_mark(ch) && begin.is_some();
            if ch.is_alphabetic() || apostrophe || combining {
                if begin.is_none() {
                    begin = Some(index);
                }
            } else if let Some(start) = begin.take() {
                results.push((
                    range.start + start..range.start + index,
                    raw[start..index].to_owned(),
                ));
            }
        }
        if let Some(start) = begin {
            results.push((range.start + start..range.end, raw[start..].to_owned()));
        }
    }
    results
}

/// Hunspell does not consistently accept elided compounds as one word. In
/// Italian and French, the part before an apostrophe can be a grammatical
/// clitic rather than an independent spelling token (l'umanità, d'accord,
/// qu'elle). Check the lexical part after a recognized elision, preserving
/// its *original byte range* for highlighting and safe replacement.
///
/// Do not split English contractions or unknown compounds: "don't" and
/// "O'Brien" must still reach their dictionary intact. A terminal apostrophe
/// (Italian po') is part of its word, not an elision.
fn spelling_words(source: &str, language: &str) -> Vec<(Range<usize>, String)> {
    words(source)
        .into_iter()
        .map(|(mut span, mut word)| {
            while let Some((apostrophe, mark)) =
                word.char_indices().find(|(_, ch)| matches!(ch, '\'' | '’'))
            {
                let prefix = &word[..apostrophe];
                let rest = &word[apostrophe + mark.len_utf8()..];
                if !is_elision_prefix(prefix, language)
                    || !rest.chars().next().is_some_and(char::is_alphabetic)
                {
                    break;
                }
                let shift = apostrophe + mark.len_utf8();
                span.start += shift;
                word = rest.to_owned();
            }
            (span, word)
        })
        .collect()
}

fn is_elision_prefix(prefix: &str, language: &str) -> bool {
    let allowed: &[&str] = match language {
        "it_IT" => &[
            "l", "d", "all", "dall", "dell", "nell", "sull", "un", "quest", "quell", "c", "m", "t",
            "s", "v", "gl", "ch", "anch", "senz", "com", "cos", "dov", "bell", "grand", "mezz",
            "sant", "nessun", "tutt", "gliel",
        ],
        "fr_FR" => &[
            "l", "d", "j", "m", "t", "s", "n", "c", "qu", "jusqu", "lorsqu", "puisqu", "quelqu",
            "entr", "presqu", "quoiqu",
        ],
        _ => return false,
    };
    allowed
        .iter()
        .any(|candidate| prefix.eq_ignore_ascii_case(candidate))
}

pub fn scan_document(
    document: DocumentId,
    region: usize,
    source: &str,
    language: &str,
    archive_root: &Path,
    dictionary: &DictionaryLocation,
) -> Result<Vec<SpellIssue>, String> {
    if !valid_language(language) {
        return Err(format!("invalid language {language}"));
    }
    let words = spelling_words(source, language);
    if words.is_empty() {
        return Ok(Vec::new());
    }
    let mut command = Command::new("hunspell");
    command.arg("-a").arg("-i").arg("UTF-8").arg("-d");
    match dictionary {
        DictionaryLocation::System => {
            command.arg(language);
        }
        DictionaryLocation::Managed(base) => {
            command.arg(base);
        }
    }
    let personal = archive_root
        .join("spelling")
        .join(format!("{language}.dic"));
    if personal.is_file() {
        command.arg("-p").arg(personal);
    }
    let mut child = command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| format!("cannot start hunspell: {error}"))?;
    // Drain stdout while feeding stdin: large Documents must not deadlock on full pipes.
    let mut input = child.stdin.take().ok_or("cannot open Hunspell input")?;
    let input_words = words
        .iter()
        .map(|(_, word)| word.clone())
        .collect::<Vec<_>>();
    let writer = std::thread::spawn(move || -> std::io::Result<()> {
        for word in input_words {
            writeln!(input, "{word}")?;
        }
        Ok(())
    });
    let output = child
        .wait_with_output()
        .map_err(|error| format!("hunspell process: {error}"))?;
    let written = writer
        .join()
        .map_err(|_| "Hunspell writer panicked".to_owned())?;
    if !output.status.success() {
        return Err(format!(
            "hunspell failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    written.map_err(|error| format!("hunspell input: {error}"))?;
    let response = String::from_utf8(output.stdout)
        .map_err(|_| "hunspell returned invalid UTF-8".to_owned())?;
    let mut lines = response.lines();
    let header = lines.next().unwrap_or("");
    if !header.starts_with("@(#)") && !header.starts_with("Hunspell") {
        return Err("unexpected Hunspell pipe header".into());
    }
    let mut issues = Vec::new();
    for (span, word) in words {
        let replies = read_hunspell_replies(&mut lines, &word)?;
        let Some(suggestions) = classify_hunspell_replies(&word, &replies)? else {
            continue;
        };
        issues.push(SpellIssue {
            document,
            region,
            language: language.to_owned(),
            start: span.start,
            end: span.end,
            word,
            suggestions,
        });
    }
    Ok(issues)
}

/// Hunspell -a emits one status for *each token* recognized on an input line,
/// then a blank line. One input word is not guaranteed to be one Hunspell
/// token: dictionary-specific WORDCHARS, apostrophes and Unicode may split it.
fn read_hunspell_replies<'a>(
    lines: &mut impl Iterator<Item = &'a str>,
    word: &str,
) -> Result<Vec<&'a str>, String> {
    let first = lines
        .by_ref()
        .find(|line| !line.trim().is_empty())
        .ok_or_else(|| format!("hunspell response truncated at {word}"))?;
    let mut replies = vec![first];
    for line in lines {
        if line.trim().is_empty() {
            break;
        }
        replies.push(line);
    }
    Ok(replies)
}

/// Returns None for an accepted word, Some(suggestions) for a spelling issue.
/// Suggestions are only safe when Hunspell checked the complete input as a
/// single token. A compound/split response must never supply a replacement
/// for one fragment as though it replaced the author's entire word.
fn classify_hunspell_replies(word: &str, replies: &[&str]) -> Result<Option<Vec<String>>, String> {
    let mut misspelled = false;
    for reply in replies {
        if reply.starts_with("& ") || reply.starts_with("? ") || reply.starts_with("# ") {
            misspelled = true;
        } else if !matches!(reply.chars().next(), Some('*' | '+' | '-')) {
            return Err(format!("unexpected Hunspell response for {word}: {reply}"));
        }
    }
    if !misspelled {
        return Ok(None);
    }
    if replies.len() != 1 {
        // At least one fragment failed, but suggestions and offsets belong to
        // individual fragments rather than to this complete source word.
        return Ok(Some(Vec::new()));
    }
    let reply = replies[0];
    let suggestions = if reply.starts_with("& ") || reply.starts_with("? ") {
        reply
            .split_once(": ")
            .map(|(_, alternatives)| {
                alternatives
                    .split(", ")
                    .filter(|part| !part.is_empty())
                    .take(6)
                    .map(str::to_owned)
                    .collect()
            })
            .unwrap_or_default()
    } else {
        Vec::new()
    };
    Ok(Some(suggestions))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn italian_interfaccia_with_multiple_hunspell_statuses_is_not_unavailable() {
        // A dictionary is allowed to tokenize one supplied word into more
        // than one token. All accepted fragments mean no spelling issue.
        assert_eq!(
            classify_hunspell_replies("interfaccia", &["*", "+ inter", "*"]).unwrap(),
            None
        );
    }

    #[test]
    fn hunspell_split_word_does_not_reuse_fragment_suggestions() {
        let result =
            classify_hunspell_replies("l’éléphant", &["*", "& éléphnt 2 2: éléphant, éléphante"])
                .unwrap();
        assert_eq!(result, Some(Vec::new()));
    }

    #[test]
    fn hunspell_multiline_group_keeps_next_word_aligned() {
        let response = "@(#) Hunspell 1.7\n*\n*\n\n& erore 1 0: errore\n\n*\n\n";
        let mut lines = response.lines();
        assert!(lines.next().unwrap().starts_with("@(#)"));
        let multiple = read_hunspell_replies(&mut lines, "interfaccia").unwrap();
        assert_eq!(multiple, vec!["*", "*"]);
        assert_eq!(
            classify_hunspell_replies("interfaccia", &multiple).unwrap(),
            None
        );
        let typo = read_hunspell_replies(&mut lines, "erore").unwrap();
        assert_eq!(
            classify_hunspell_replies("erore", &typo).unwrap(),
            Some(vec!["errore".to_owned()])
        );
        let good = read_hunspell_replies(&mut lines, "testo").unwrap();
        assert_eq!(classify_hunspell_replies("testo", &good).unwrap(), None);
    }

    #[test]
    fn hunspell_unexpected_reply_still_reports_protocol_error() {
        assert!(classify_hunspell_replies("interfaccia", &["UNEXPECTED"]).is_err());
    }

    #[test]
    fn locale_validation() {
        assert!(valid_language("it_IT"));
        assert!(valid_language("fr_FR"));
        assert!(!valid_language("../../"));
        assert!(!valid_language("it_IT.UTF-8"));
    }

    #[test]
    fn safe_markdown_ranges() {
        let text =
            "# Buongiorno\n\nTesto con [link](https://example.com/zzzzz) e ~~~codicezz~~~.\n";
        let tokens = words(text)
            .into_iter()
            .map(|(_, word)| word)
            .collect::<Vec<_>>();
        assert!(tokens.contains(&"Buongiorno".to_owned()));
        assert!(tokens.contains(&"link".to_owned()));
        assert!(!tokens.contains(&"example".to_owned()));
    }

    #[test]
    fn unicode_source_offsets() {
        let text = "L'éléphant è qui.";
        for (range, word) in words(text) {
            assert_eq!(&text[range], word);
        }
    }

    #[test]
    fn code_blocks_are_not_prose() {
        let text = "~~~rust\nsyntheticcode\n~~~\n\nproseword\n\n    indentedcode\n";
        let found = words(text)
            .into_iter()
            .map(|(_, word)| word)
            .collect::<Vec<_>>();
        assert!(found.contains(&"proseword".to_owned()));
        assert!(!found.contains(&"syntheticcode".to_owned()));
        assert!(!found.contains(&"indentedcode".to_owned()));
    }

    #[test]
    fn italian_trailing_apostrophe_is_part_of_word() {
        let source = "po' po’ l'éléphant l’éléphant";
        let found = words(source);
        let terms = found
            .iter()
            .map(|(_, word)| word.as_str())
            .collect::<Vec<_>>();
        assert_eq!(terms, vec!["po'", "po’", "l'éléphant", "l’éléphant"]);
        for (range, word) in found {
            assert_eq!(&source[range], word);
        }
    }

    #[test]
    fn italian_apostrophe_elisions_check_the_lexical_word_only() {
        let source = "l'umanità l’umanità dell'amore un'amica c'è quest'anno l'ummanità";
        let found = spelling_words(source, "it_IT");
        let terms = found
            .iter()
            .map(|(_, word)| word.as_str())
            .collect::<Vec<_>>();
        assert_eq!(
            terms,
            vec![
                "umanità",
                "umanità",
                "amore",
                "amica",
                "è",
                "anno",
                "ummanità"
            ]
        );
        for (range, word) in found {
            assert_eq!(&source[range], word);
        }
    }

    #[test]
    fn italian_apostrophe_keeps_truncations_and_unknown_compounds() {
        let source = "po' po’ rock'n'roll l''umanità";
        let terms = spelling_words(source, "it_IT")
            .into_iter()
            .map(|(_, word)| word)
            .collect::<Vec<_>>();
        assert_eq!(terms, vec!["po'", "po’", "rock'n'roll", "l''umanità"]);
    }

    #[test]
    fn french_elisions_split_but_english_contractions_remain_intact() {
        let source = "l'éléphant qu’elle n'est";
        let terms = spelling_words(source, "fr_FR")
            .into_iter()
            .map(|(_, word)| word)
            .collect::<Vec<_>>();
        assert_eq!(terms, vec!["éléphant", "elle", "est"]);

        let english = "don't it's O'Brien";
        let terms = spelling_words(english, "en_GB")
            .into_iter()
            .map(|(_, word)| word)
            .collect::<Vec<_>>();
        assert_eq!(terms, vec!["don't", "it's", "O'Brien"]);
    }

    #[test]
    fn elision_preserves_unicode_byte_spans_and_combining_accents() {
        let source = "L’umanita\u{300} l'umanità l’un'amica";
        let found = spelling_words(source, "it_IT");
        assert_eq!(found[0].1, "umanita\u{300}");
        assert_eq!(found[0].0.start, "L’".len());
        assert_eq!(found[1].1, "umanità");
        assert_eq!(found[2].1, "amica");
        for (range, word) in found {
            assert_eq!(&source[range], word);
        }
    }

    #[test]
    fn decomposed_unicode_marks_remain_inside_replacement_span() {
        let source = "cafe\u{301} cafe\u{301}\u{308} élan";
        let found = words(source);
        assert_eq!(found[0].1, "cafe\u{301}");
        assert_eq!(found[0].0, 0..6);
        assert_eq!(found[1].1, "cafe\u{301}\u{308}");
        for (range, word) in found {
            assert_eq!(&source[range], word);
        }
    }
}
