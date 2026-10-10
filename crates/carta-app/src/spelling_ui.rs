//! Shared spelling commands and selector workflow, independent of TUI/GUI.
use super::*;
use crate::spelling::{self, SpellIssue};
use std::collections::HashSet;

pub(super) struct SpellSession {
    issues: Vec<SpellIssue>,
    next: usize,
    ignored: HashSet<(String, String)>,
}

impl App {
    fn document_language(&self, document: DocumentId) -> String {
        self.archive
            .document_info(document)
            .and_then(|info| info.metadata().language())
            .filter(|language| spelling::valid_language(language))
            .map(str::to_owned)
            .unwrap_or_else(spelling::system_language)
    }

    pub(super) fn select_document_language(&mut self) -> AppResult {
        self.current_document()?;
        let languages = [
            ("System default", ""),
            ("Italian (it_IT)", "it_IT"),
            ("French (fr_FR)", "fr_FR"),
            ("English UK (en_GB)", "en_GB"),
            ("English US (en_US)", "en_US"),
            ("German (de_DE)", "de_DE"),
            ("Spanish (es_ES)", "es_ES"),
        ];
        self.mode = AppMode::Selector {
            title: "Document spelling language".into(),
            query: String::new(),
            selected: 0,
            choices: languages
                .into_iter()
                .map(|(label, value)| Choice {
                    label: label.into(),
                    value: value.into(),
                })
                .collect(),
            action: SelectAction::SetDocumentLanguage,
        };
        Ok(())
    }

    pub(super) fn apply_document_language(&mut self, selected: &str) -> AppResult {
        self.autosave()?;
        let document = self.current_document()?;
        self.archive
            .set_document_language(document, (!selected.is_empty()).then_some(selected))?;
        self.checkpoint(
            CheckpointKind::Structural,
            Some("Changed Document spelling language"),
        )?;
        self.status = if selected.is_empty() {
            format!(
                "Document language: system ({})",
                spelling::system_language()
            )
        } else {
            format!("Document language: {selected}")
        };
        Ok(())
    }

    pub(super) fn select_dictionary_word(&mut self) -> AppResult {
        let language = self.document_language(self.current_document()?);
        let choices = self
            .archive
            .personal_words(&language)?
            .into_iter()
            .map(|word| Choice {
                label: word.clone(),
                value: word,
            })
            .collect::<Vec<_>>();
        if choices.is_empty() {
            self.status = format!("No personal words for {language}");
            return Ok(());
        }
        self.mode = AppMode::Selector {
            title: format!("Remove personal word ({language})"),
            query: String::new(),
            selected: 0,
            choices,
            action: SelectAction::RemoveDictionaryWord,
        };
        Ok(())
    }

    pub(super) fn remove_dictionary_word(&mut self, word: &str) -> AppResult {
        let language = self.document_language(self.current_document()?);
        self.autosave()?;
        if self.archive.remove_personal_word(&language, word)? {
            self.checkpoint(
                CheckpointKind::Structural,
                Some("Removed personal spelling word"),
            )?;
        }
        self.status = format!("Removed {word} from {language}");
        Ok(())
    }

    pub(super) fn start_spelling(&mut self, work: bool) -> AppResult {
        if work && !matches!(self.view, View::Work(_)) {
            self.status = "Check Work Spelling requires Work View".into();
            return Ok(());
        }
        let current = self.editor.cursor().region;
        let mut issues = Vec::new();
        for (index, region) in self.editor.regions().iter().enumerate() {
            if !work && index != current {
                continue;
            }
            let language = self.document_language(region.document);
            match spelling::scan_document(
                region.document,
                index,
                &region.text,
                &language,
                self.archive.root(),
            ) {
                Ok(found) => issues.extend(found),
                Err(error) => {
                    self.status = format!("Spelling unavailable ({language}): {error}");
                    return Ok(());
                }
            }
        }
        self.cat_navigation();
        self.spelling = Some(SpellSession {
            issues,
            next: 0,
            ignored: HashSet::new(),
        });
        self.advance_spelling()
    }

    pub(super) fn advance_spelling(&mut self) -> AppResult {
        loop {
            let Some(session) = self.spelling.as_mut() else {
                return Ok(());
            };
            if session.next == session.issues.len() {
                self.spelling = None;
                self.editor.clear_cat_highlight();
                self.mode = AppMode::Editing;
                self.status = "Spelling check complete".into();
                return Ok(());
            }
            let issue = session.issues[session.next].clone();
            if session
                .ignored
                .contains(&(issue.language.clone(), issue.word.clone()))
            {
                session.next += 1;
                continue;
            }
            let Some(region) = self.editor.regions().get(issue.region) else {
                session.next += 1;
                continue;
            };
            if region.document != issue.document
                || region.text.get(issue.start..issue.end) != Some(issue.word.as_str())
            {
                session.next += 1;
                continue;
            }
            let first = Cursor {
                region: issue.region,
                byte: issue.start,
            };
            let last = Cursor {
                region: issue.region,
                byte: issue.end,
            };
            self.editor.set_cursor(first, false);
            self.editor.set_cat_highlight(first, last);
            let mut choices = issue
                .suggestions
                .iter()
                .enumerate()
                .map(|(index, suggestion)| Choice {
                    label: suggestion.clone(),
                    value: format!("replace:{index}"),
                })
                .collect::<Vec<_>>();
            choices.extend([
                Choice {
                    label: "Ignore once".into(),
                    value: "ignore".into(),
                },
                Choice {
                    label: "Ignore all (this check)".into(),
                    value: "ignore-all".into(),
                },
                Choice {
                    label: "Add to personal dictionary".into(),
                    value: "add".into(),
                },
            ]);
            let remaining = session.issues.len() - session.next;
            self.mode = AppMode::Selector {
                title: format!(
                    "Spelling: {} ({}, {remaining} remaining)",
                    issue.word, issue.language
                ),
                query: String::new(),
                selected: 0,
                choices,
                action: SelectAction::SpellingSuggestion,
            };
            return Ok(());
        }
    }

    pub(super) fn apply_spelling_choice(&mut self, choice: &str) -> AppResult {
        let Some(issue) = self
            .spelling
            .as_ref()
            .and_then(|session| session.issues.get(session.next))
            .cloned()
        else {
            return Ok(());
        };
        if let Some(index) = choice.strip_prefix("replace:") {
            let index: usize = index.parse()?;
            if let Some(replacement) = issue.suggestions.get(index) {
                if self.archive.document_is_locked(issue.document)? {
                    self.status = "Document locked: cannot replace spelling".into();
                } else {
                    let start = Cursor {
                        region: issue.region,
                        byte: issue.start,
                    };
                    let end = Cursor {
                        region: issue.region,
                        byte: issue.end,
                    };
                    if self.editor.set_cat_highlight(start, end)
                        && self.editor.replace_cat_highlight(replacement)
                    {
                        let diff = replacement.len() as isize - issue.word.len() as isize;
                        if let Some(session) = self.spelling.as_mut() {
                            for later in session.issues.iter_mut().skip(session.next + 1) {
                                if later.region == issue.region && later.start >= issue.end {
                                    later.start = later.start.saturating_add_signed(diff);
                                    later.end = later.end.saturating_add_signed(diff);
                                }
                            }
                        }
                        self.edited(Instant::now());
                    }
                }
            }
        } else if choice == "ignore-all" {
            if let Some(session) = self.spelling.as_mut() {
                session
                    .ignored
                    .insert((issue.language.clone(), issue.word.clone()));
            }
        } else if choice == "add" {
            self.autosave()?;
            if self
                .archive
                .add_personal_word(&issue.language, &issue.word)?
            {
                self.checkpoint(
                    CheckpointKind::Structural,
                    Some("Added personal spelling word"),
                )?;
            }
            if let Some(session) = self.spelling.as_mut() {
                session
                    .ignored
                    .insert((issue.language.clone(), issue.word.clone()));
            }
        }
        if let Some(session) = self.spelling.as_mut() {
            session.next += 1;
        }
        self.advance_spelling()
    }
}
