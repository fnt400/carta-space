//! Optional on-demand Hunspell service; only reports candidate source spans.
use std::io::Write;
use std::ops::Range;
use std::path::Path;
use std::process::{Command, Stdio};

use carta_core::DocumentId;
use pulldown_cmark::{Event, Parser};

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
    for (event, range) in Parser::new(source).into_offset_iter() {
        let Event::Text(rendered) = event else {
            continue;
        };
        let Some(raw) = source.get(range.clone()) else {
            continue;
        };
        if raw != rendered.as_ref() {
            continue;
        }
        let mut begin = None;
        let mut chars = raw.char_indices().peekable();
        while let Some((index, ch)) = chars.next() {
            let joiner = matches!(ch, '\'' | '’')
                && begin.is_some()
                && chars.peek().is_some_and(|(_, next)| next.is_alphabetic());
            if ch.is_alphabetic() || joiner {
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

pub fn scan_document(
    document: DocumentId,
    region: usize,
    source: &str,
    language: &str,
    archive_root: &Path,
) -> Result<Vec<SpellIssue>, String> {
    if !valid_language(language) {
        return Err(format!("invalid language {language}"));
    }
    let words = words(source);
    if words.is_empty() {
        return Ok(Vec::new());
    }
    let mut command = Command::new("hunspell");
    command
        .arg("-a")
        .arg("-i")
        .arg("UTF-8")
        .arg("-d")
        .arg(language);
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
        let output = lines
            .by_ref()
            .find(|line| !line.trim().is_empty())
            .ok_or_else(|| format!("hunspell response truncated at {word}"))?;
        // Hunspell terminates each input line with a blank output line.
        // Refuse ambiguous splits rather than assign suggestions to the wrong word.
        let mut extra = 0;
        for response in lines.by_ref() {
            if response.trim().is_empty() { break; }
            extra += 1;
        }
        if extra != 0 {
            return Err(format!("Hunspell split {word} into multiple tokens"));
        }
        if matches!(output.chars().next(), Some('*' | '+' | '-')) {
            continue;
        }
        let suggestions = if output.starts_with("& ") || output.starts_with("? ") {
            output
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
        } else if output.starts_with("# ") {
            Vec::new()
        } else {
            return Err(format!("unexpected Hunspell response for {word}"));
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

#[cfg(test)]
mod tests {
    use super::*;

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
}
