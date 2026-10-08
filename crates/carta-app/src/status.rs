use chrono::Local;

use crate::help;
use crate::{App, HelpKind, View};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatusBar {
    pub left: String,
    pub right: String,
}

impl App {
    pub fn status_bar(&self) -> StatusBar {
        StatusBar {
            left: status_left(self),
            right: status_right(self),
        }
    }
}

fn status_left(app: &App) -> String {
    let flags = status_flags(app);
    if !app.status.is_empty() {
        return if flags.is_empty() {
            app.status.clone()
        } else {
            format!("{flags} · {}", app.status)
        };
    }

    let base = match &app.view {
        View::CreationDate(volume) => format!(
            "{} · {} · {:04}-{:02} · {}",
            current_document_date(app),
            current_label(app),
            volume.year(),
            volume.month(),
            current_position(app)
        ),
        View::ModificationDate => format!(
            "{} · {} · {}",
            current_document_modified_date(app),
            current_label(app),
            current_position(app)
        ),
        View::Work(_) => format!(
            "{} · {} · {}",
            current_document_date(app),
            current_label(app),
            current_position(app)
        ),
        View::Search { query, selected } => {
            let date = app
                .search_results
                .get(*selected)
                .map_or_else(|| "No date".to_owned(), |row| date_only(row.created));
            format!(
                "{} · Search: {} · {}/{}",
                date,
                query,
                if app.search_results.is_empty() {
                    0
                } else {
                    selected + 1
                },
                app.search_results.len()
            )
        }
        View::History { selected, .. } => app.history.get(*selected).map_or_else(
            || "History · 0/0".to_owned(),
            |revision| {
                format!(
                    "{} · History · {} · {}",
                    date_only(revision.checkpoint().created()),
                    revision.document().derived_label(),
                    list_position(*selected, app.history.len())
                )
            },
        ),
        View::WorkHistory { selected, .. } => app.work_history.get(*selected).map_or_else(
            || "Work History · 0/0".to_owned(),
            |snapshot| {
                format!(
                    "{} · Work History · {} · {}",
                    date_only(snapshot.checkpoint().created()),
                    snapshot.title(),
                    list_position(*selected, app.work_history.len())
                )
            },
        ),
        View::Trash { selected } => {
            if let Some(trash) = &app.trash {
                let total = trash.documents().len() + trash.works().len();
                if let Some(document) = trash.documents().get(*selected) {
                    format!(
                        "{} · Trash · Document: {} · {}",
                        date_only(document.created()),
                        document.label(),
                        list_position(*selected, total)
                    )
                } else if let Some(work) = selected
                    .checked_sub(trash.documents().len())
                    .and_then(|index| trash.works().get(index))
                {
                    format!(
                        "{} · Trash · Work: {} · {}",
                        date_only(work.created()),
                        work.title(),
                        list_position(*selected, total)
                    )
                } else {
                    "Trash · 0/0".to_owned()
                }
            } else {
                "Trash · 0/0".to_owned()
            }
        }
        View::Conflicts { .. } => format!("Conflicts · {} preserved", app.conflicts.len()),
        View::Help { kind, selected } => {
            let documents = help::documents(*kind);
            let label = match kind {
                HelpKind::Cheatsheet => "Cheatsheet",
                HelpKind::Manual => "Manual",
            };
            let title = documents
                .get(*selected)
                .map_or("Help", |document| document.title);
            format!(
                "{label} · {title} · {}",
                list_position(*selected, documents.len())
            )
        }
    };

    if flags.is_empty() {
        base
    } else {
        format!("{flags} · {base}")
    }
}

fn status_right(app: &App) -> String {
    match &app.view {
        View::CreationDate(_) => "Creation Date".to_owned(),
        View::ModificationDate => "Modification Date".to_owned(),
        View::Work(work) => app
            .archive
            .work(*work)
            .map_or_else(|| "Work".to_owned(), |work| work.title().to_owned()),
        _ => String::new(),
    }
}

fn status_flags(app: &App) -> String {
    let mut flags = Vec::new();

    if matches!(
        app.view,
        View::CreationDate(_) | View::ModificationDate | View::Work(_)
    ) {
        if let Some(document) = app.editor.current_document() {
            if app
                .archive
                .document_is_explicitly_locked(document)
                .unwrap_or(false)
            {
                flags.push("[LOCK DOC]");
            } else if app.archive.document_is_locked(document).unwrap_or(false) {
                flags.push("[LOCKED BY WORK]");
            }
        }
        if let View::Work(work) = &app.view {
            if app.archive.work_is_locked(*work).unwrap_or(false) {
                flags.push("[LOCK WORK]");
            }
        }
    }

    flags.join(" ")
}

fn current_label(app: &App) -> String {
    let Some(document) = app.editor.current_document() else {
        return "Empty View".into();
    };
    let Some(content) = app.editor.current_text() else {
        return "Empty View".into();
    };
    app.archive
        .document_info(document)
        .map_or_else(|| "Empty View".into(), |info| info.derived_label(content))
}

fn current_document_date(app: &App) -> String {
    app.editor
        .current_document()
        .and_then(|id| app.archive.document_info(id))
        .map_or_else(
            || "No date".to_owned(),
            |document| date_only(document.created()),
        )
}

fn current_document_modified_date(app: &App) -> String {
    app.editor
        .current_document()
        .and_then(|id| app.archive.document_info(id))
        .map_or_else(
            || "No date".to_owned(),
            |document| date_only(document.modified()),
        )
}

fn current_position(app: &App) -> String {
    if app.editor.regions().is_empty() {
        "0/0".to_owned()
    } else {
        format!(
            "{}/{}",
            app.editor.cursor().region + 1,
            app.editor.regions().len()
        )
    }
}

fn date_only(timestamp: carta_core::Timestamp) -> String {
    timestamp
        .as_datetime()
        .with_timezone(&Local)
        .format("%Y-%m-%d")
        .to_string()
}

fn list_position(selected: usize, total: usize) -> String {
    if total == 0 {
        "0/0".to_owned()
    } else {
        format!("{}/{}", selected + 1, total)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn list_position_handles_empty_and_selected_lists() {
        assert_eq!(list_position(0, 0), "0/0");
        assert_eq!(list_position(1, 4), "2/4");
    }
}
