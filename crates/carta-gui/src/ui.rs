use carta_app::{App, AppMode, View};
use chrono::{DateTime, Local};
use iced::theme::Mode as ThemeMode;
use iced::widget::{column, container, rich_text, row, scrollable, space, span, stack, text, Id};
use iced::{color, Color, Element, Font, Length};

use crate::presentation::SegmentKind;
use crate::viewport;
use crate::{Gui, Message};

const IOSEVKA: Font = Font::with_name("Iosevka");
const EDITOR_SIZE: f32 = viewport::FONT_SIZE;
const STATUS_SIZE: f32 = 14.0;
const EDITOR_MAX_WIDTH: f32 = viewport::PAGE_WIDTH;

pub(crate) const EDITOR_SCROLL_ID: &str = "carta-editor-scroll";

pub(crate) fn view(state: &Gui) -> Element<'_, Message> {
    if let Some(error) = &state.error {
        return container(
            column![
                text("Carta Space").font(IOSEVKA).size(24),
                text(error).font(IOSEVKA).size(16),
            ]
            .spacing(10)
            .padding(24),
        )
        .width(Length::Fill)
        .height(Length::Fill)
        .into();
    }

    let Some(app) = &state.app else {
        return container(text("Carta Space").font(IOSEVKA)).into();
    };

    let editor = editor_view(app, state.theme_mode, state.window_size);
    let leap = leap_line(app);
    let status = status_bar(app, state.theme_mode);

    let mut base = column![editor].height(Length::Fill).spacing(0);
    if let Some(leap) = leap {
        base = base.push(leap);
    }
    base = base.push(status);

    let base: Element<'_, Message> = container(base)
        .width(Length::Fill)
        .height(Length::Fill)
        .into();

    if let Some(overlay) = modal_overlay(app, state.theme_mode) {
        stack![base, overlay].into()
    } else {
        base
    }
}

fn editor_view(app: &App, mode: ThemeMode, window: iced::Size) -> Element<'_, Message> {
    let columns = viewport::columns(window.width);
    let spans = editor_spans(app, mode, columns);
    let text_view = rich_text(spans)
        .font(IOSEVKA)
        .size(EDITOR_SIZE)
        .line_height(iced::Pixels(viewport::LINE_HEIGHT))
        .width(Length::Fill);

    let page = container(text_view)
        .width(Length::Fill)
        .max_width(EDITOR_MAX_WIDTH)
        .padding([viewport::VERTICAL_PADDING as u16, viewport::HORIZONTAL_PADDING as u16]);

    let height = viewport::writing_area_height(window.height);
    let top_padding = space().height(height * (2.0 / 3.0));
    let bottom_padding = space().height(height * (2.0 / 3.0));
    let sheet = column![top_padding, container(page).center_x(Length::Fill), bottom_padding]
        .spacing(0)
        .width(Length::Fill);

    scrollable(sheet)
        .id(Id::new(EDITOR_SCROLL_ID))
        .height(Length::Fill)
        .width(Length::Fill)
        .into()
}

fn editor_spans<'a>(
    app: &'a App,
    mode: ThemeMode,
    columns: usize,
) -> Vec<iced::widget::text::Span<'a, ()>> {
    let mut spans = Vec::new();
    let cursor = app.editor.cursor();
    let selection = app
        .cat_render_highlight()
        .or_else(|| app.editor.selection());
    let extended = app.editor.cat_highlight().is_some() || app.editor.selection().is_some();

    for (region_index, region) in app.editor.regions().iter().enumerate() {
        if let Some(separator) = separator_for_region(app, region_index, columns) {
            if region_index > 0 {
                spans.push(span("\n"));
            }
            spans.push(span(separator).font(IOSEVKA).color(secondary_text(mode)));
            spans.push(span("\n\n"));
        } else if region_index > 0 {
            spans.push(span("\n\n"));
        }

        if region_index == cursor.region {
            let selected = selection.and_then(|(start, end)| {
                (start.region == region_index && end.region == region_index)
                    .then_some((start.byte, end.byte))
            });

            spans.extend(
                crate::presentation::segments(&region.text, cursor.byte, selected)
                    .into_iter()
                    .flat_map(|segment| {
                        let newline_caret =
                            segment.kind == SegmentKind::Caret && segment.content == "\n";
                        let styled = match segment.kind {
                            SegmentKind::Text => span(segment.content).font(IOSEVKA),
                            SegmentKind::Selection => span(segment.content)
                                .font(IOSEVKA)
                                .background(if extended {
                                    selection_background(mode)
                                } else {
                                    cat_highlight_background(mode)
                                })
                                .color(selection_foreground(mode)),
                            SegmentKind::Caret => {
                                // A newline has no printable cell, so give its caret
                                // a stable one-cell placeholder before the LF.
                                let content =
                                    if segment.content.is_empty() || segment.content == "\n" {
                                        " "
                                    } else {
                                        segment.content
                                    };
                                let caret = span(content).font(IOSEVKA);
                                caret
                                    .background(caret_color(mode))
                                    .color(caret_foreground(mode))
                            }
                        };
                        if newline_caret {
                            vec![styled, span("\n").font(IOSEVKA)]
                        } else {
                            vec![styled]
                        }
                    }),
            );
        } else {
            spans.push(span(region.text.as_str()).font(IOSEVKA));
        }
    }

    if spans.is_empty() {
        let caret = span(" ").font(IOSEVKA);
        spans.push(
            caret
                .background(caret_color(mode))
                .color(caret_foreground(mode)),
        );
    }

    spans
}

fn separator_for_region(app: &App, region_index: usize, columns: usize) -> Option<String> {
    let region = app.editor.regions().get(region_index)?;
    let info = app.archive.document_info(region.document)?;
    let locked = app
        .archive
        .document_is_locked(region.document)
        .unwrap_or(false);
    let marker = if locked { "[LOCKED] " } else { "" };

    match &app.view {
        View::CreationDate(_) => Some(format!(
            "── {marker}{} ──────────────────────────────",
            local_date_time(info.created())
        )),
        View::ModificationDate => Some(format!(
            "── {marker}{} ──────────────────────────────",
            local_date_time(info.modified())
        )),
        View::Work(_) if region_index > 0 || locked => Some(work_rule(columns, locked)),
        _ => None,
    }
}

fn work_rule(columns: usize, locked: bool) -> String {
    let prefix = if locked { "[LOCKED] " } else { "" };
    format!("{prefix}{}", "─".repeat(columns.saturating_sub(prefix.chars().count())))
}

fn local_date_time(timestamp: carta_core::Timestamp) -> String {
    let date: DateTime<Local> = timestamp.as_datetime().with_timezone(&Local);
    date.format("%a %Y-%m-%d %H:%M").to_string()
}

fn leap_line(app: &App) -> Option<Element<'_, Message>> {
    let AppMode::Leap { session, .. } = &app.mode else {
        return None;
    };
    let direction = match session.direction() {
        carta_core::LeapDirection::Forward => "Forward",
        carta_core::LeapDirection::Backward => "Backward",
    };
    Some(
        container(
            text(format!("LEAP {direction}: {}", session.query()))
                .font(IOSEVKA)
                .size(STATUS_SIZE),
        )
        .padding([3, 10])
        .width(Length::Fill)
        .into(),
    )
}

fn status_bar(app: &App, mode: ThemeMode) -> Element<'_, Message> {
    let status = app.status_bar();
    let right = text(status.right).font(IOSEVKA).size(STATUS_SIZE);
    let line = row![
        text(status.left)
            .font(IOSEVKA)
            .size(STATUS_SIZE)
            .width(Length::Fill),
        right,
    ]
    .spacing(12);

    container(line)
        .padding([3, 9])
        .width(Length::Fill)
        .style(move |_| {
            iced::widget::container::Style::default()
                .background(status_background(mode))
                .color(status_foreground(mode))
        })
        .into()
}

fn modal_overlay(app: &App, mode: ThemeMode) -> Option<Element<'_, Message>> {
    let panel: Element<'_, Message> = match &app.mode {
        AppMode::Editing | AppMode::Leap { .. } => return None,
        AppMode::Palette { query, selected } => {
            let commands = app.palette_commands(query);
            let mut content =
                column![text(format!("> {query}")).font(IOSEVKA).size(EDITOR_SIZE),].spacing(4);
            if commands.is_empty() {
                content =
                    content.push(text("No matching commands").font(IOSEVKA).size(EDITOR_SIZE));
            } else {
                for (index, command) in commands.iter().enumerate().take(14) {
                    let marker = if index == *selected { "▶ " } else { "  " };
                    content = content.push(
                        text(format!("{marker}{}", command.label()))
                            .font(IOSEVKA)
                            .size(EDITOR_SIZE),
                    );
                }
            }
            modal_box(content, mode)
        }
        AppMode::Selector {
            title,
            query,
            selected,
            choices,
            ..
        } => {
            let matches: Vec<_> = choices
                .iter()
                .filter(|choice| carta_app::palette::matches(query, &choice.label))
                .collect();
            let mut content = column![text(format!("{title}: {query}"))
                .font(IOSEVKA)
                .size(EDITOR_SIZE),]
            .spacing(4);
            for (index, choice) in matches.iter().enumerate().take(14) {
                let marker = if index == *selected { "▶ " } else { "  " };
                content = content.push(
                    text(format!("{marker}{}", choice.label))
                        .font(IOSEVKA)
                        .size(EDITOR_SIZE),
                );
            }
            modal_box(content, mode)
        }
        AppMode::Prompt {
            title,
            input,
            cursor,
            details,
            ..
        } => {
            let mut content = column![text(title).font(IOSEVKA).size(EDITOR_SIZE),].spacing(7);
            for detail in details {
                content = content.push(text(detail).font(IOSEVKA).size(STATUS_SIZE));
            }
            content = content.push(
                text(format!("> {}", with_caret(input, *cursor)))
                    .font(IOSEVKA)
                    .size(EDITOR_SIZE),
            );
            modal_box(content, mode)
        }
        AppMode::Confirm { title, details, .. } => {
            let mut content = column![text(title)
                .font(IOSEVKA)
                .size(EDITOR_SIZE)
                .color(color!(0xFF7777)),]
            .spacing(7);
            for detail in details {
                content = content.push(text(detail).font(IOSEVKA).size(STATUS_SIZE));
            }
            content = content.push(
                text("Press y to confirm; n or Esc to cancel.")
                    .font(IOSEVKA)
                    .size(STATUS_SIZE),
            );
            modal_box(content, mode)
        }
    };

    Some(container(panel).center(Length::Fill).into())
}

fn modal_box<'a>(
    content: iced::widget::Column<'a, Message>,
    mode: ThemeMode,
) -> Element<'a, Message> {
    container(content)
        .padding(16)
        .max_width(700)
        .style(move |_| {
            iced::widget::container::Style::default()
                .background(modal_background(mode))
                .color(modal_foreground(mode))
                .border(iced::Border {
                    color: secondary_text(mode),
                    width: 1.0,
                    radius: 0.0.into(),
                })
        })
        .into()
}

fn with_caret(input: &str, cursor: usize) -> String {
    let mut cursor = cursor.min(input.len());
    while !input.is_char_boundary(cursor) {
        cursor -= 1;
    }
    let mut value = input.to_owned();
    value.insert(cursor, '▏');
    value
}

fn secondary_text(mode: ThemeMode) -> Color {
    match mode {
        ThemeMode::Light => color!(0x666666),
        ThemeMode::Dark | ThemeMode::None => color!(0x888888),
    }
}

fn caret_color(mode: ThemeMode) -> Color {
    match mode {
        ThemeMode::Light => color!(0x333333),
        ThemeMode::Dark | ThemeMode::None => color!(0xE0E0E0),
    }
}

fn caret_foreground(mode: ThemeMode) -> Color {
    match mode {
        ThemeMode::Light => color!(0xFFFFFF),
        ThemeMode::Dark | ThemeMode::None => color!(0x111111),
    }
}

fn cat_highlight_background(mode: ThemeMode) -> Color {
    match mode {
        ThemeMode::Light => color!(0xDDDDDD),
        ThemeMode::Dark | ThemeMode::None => color!(0x555555),
    }
}

fn selection_background(mode: ThemeMode) -> Color {
    match mode {
        ThemeMode::Light => color!(0xBFD5F2),
        ThemeMode::Dark | ThemeMode::None => color!(0x294B6A),
    }
}

fn selection_foreground(mode: ThemeMode) -> Color {
    match mode {
        ThemeMode::Light => color!(0x182C45),
        ThemeMode::Dark | ThemeMode::None => color!(0xF2F2F2),
    }
}

fn status_background(mode: ThemeMode) -> Color {
    match mode {
        ThemeMode::Light => color!(0xD0D0D0),
        ThemeMode::Dark | ThemeMode::None => color!(0x666666),
    }
}

fn status_foreground(mode: ThemeMode) -> Color {
    match mode {
        ThemeMode::Light => color!(0x202020),
        ThemeMode::Dark | ThemeMode::None => color!(0xFFFFFF),
    }
}

fn modal_background(mode: ThemeMode) -> Color {
    match mode {
        ThemeMode::Light => color!(0xF4F4F4),
        ThemeMode::Dark | ThemeMode::None => color!(0x181818),
    }
}

fn modal_foreground(mode: ThemeMode) -> Color {
    match mode {
        ThemeMode::Light => color!(0x202020),
        ThemeMode::Dark | ThemeMode::None => color!(0xD5D5D5),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn work_rule_is_continuous_at_each_window_width() {
        assert_eq!(work_rule(40, false), "─".repeat(40));
        assert_eq!(work_rule(80, true).chars().count(), 80);
        assert!(work_rule(80, true).starts_with("[LOCKED] "));
        assert!(!work_rule(40, false).contains('\n'));
    }

    #[test]
    fn prompt_caret_is_utf8_safe() {
        assert_eq!(with_caret("caffè", 4), "caff▏è");
        assert_eq!(with_caret("é", 1), "▏é");
        assert_eq!(with_caret("é", 2), "é▏");
    }
}
