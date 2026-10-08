use carta_app::{App, AppMode, View};
use chrono::{DateTime, Local};
use iced::theme::Mode as ThemeMode;
use iced::widget::{column, container, mouse_area, rich_text, row, scrollable, space, span, stack, text, Id};
use iced::{color, Color, Element, Font, Length};
use std::time::Instant;

use crate::markdown::{self, Syntax};
use crate::viewport;
use crate::{Gui, Message};

const IOSEVKA: Font = Font::with_name("Iosevka");
const EDITOR_SIZE: f32 = viewport::FONT_SIZE; // Modal palette typography.
const STATUS_SIZE: f32 = 14.0;
const EDITOR_MAX_WIDTH: f32 = viewport::PAGE_WIDTH;

pub(crate) const EDITOR_SCROLL_ID: &str = "carta-editor-scroll";

pub(crate) fn view(state: &Gui) -> Element<'_, Message> {
    let started = crate::profile::enabled().then(Instant::now);
    let result = build_view(state);
    if let Some(started) = started {
        crate::profile::record("view", started);
    }
    result
}

fn build_view(state: &Gui) -> Element<'_, Message> {
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

    let editor = editor_view(state, app, state.theme_mode, state.window_size);
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


fn editor_view<'a>(state: &Gui, app: &'a App, mode: ThemeMode, window: iced::Size) -> Element<'a, Message> {
    let columns = viewport::columns(window.width, state.font_size);
    let spans = editor_spans(app, mode, columns, &mut state.markdown.borrow_mut());
    let text_view = rich_text(spans)
        .font(IOSEVKA)
        .size(state.font_size)
        .line_height(iced::Pixels(viewport::line_height(state.font_size)))
        .width(Length::Fill);

    let page = container(text_view)
        .width(Length::Fill)
        .max_width(EDITOR_MAX_WIDTH)
        .padding([
            viewport::VERTICAL_PADDING as u16,
            viewport::HORIZONTAL_PADDING as u16,
        ]);

    // The mouse area has the same page-row origin used by the hit tester.
    // It does not include the virtual padding at the top of the scroll sheet.
    let area = mouse_area(container(page).center_x(Length::Fill))
        .on_move(Message::PointerMoved)
        .on_press(Message::PointerPressed)
        .on_release(Message::PointerReleased)
        .interaction(iced::mouse::Interaction::Text);

    let height = viewport::writing_area_height(window.height);
    let top_padding = space().height(height * (2.0 / 3.0));
    let bottom_padding = space().height(height * (2.0 / 3.0));
    let sheet = column![top_padding, area, bottom_padding]
        .spacing(0)
        .width(Length::Fill);

    scrollable(sheet)
        .id(Id::new(EDITOR_SCROLL_ID))
        .height(Length::Fill)
        .width(Length::Fill)
        .into()
}

fn syntax_color(mode: ThemeMode, syntax: Syntax) -> Color {
    // Source Markdown remains visible: only ink color changes, never metrics.
    match (mode, syntax) {
        (ThemeMode::Light, Syntax::Heading) => color!(0x7940AC),
        (ThemeMode::Light, Syntax::Strong) => color!(0xA04A23),
        (ThemeMode::Light, Syntax::Emphasis) => color!(0x276783),
        (ThemeMode::Light, Syntax::Link) => color!(0x175BBD),
        (ThemeMode::Light, Syntax::Code) => color!(0x317542),
        (ThemeMode::Light, Syntax::Quote) => color!(0x67807B),
        (_, Syntax::Heading) => color!(0xC5A1FB),
        (_, Syntax::Strong) => color!(0xFFD098),
        (_, Syntax::Emphasis) => color!(0x95D8E7),
        (_, Syntax::Link) => color!(0x8DBAFF),
        (_, Syntax::Code) => color!(0xA5E5B0),
        (_, Syntax::Quote) => color!(0x95AAA7),
    }
}

fn editor_spans<'a>(
    app: &'a App,
    mode: ThemeMode,
    columns: usize,
    cache: &mut markdown::Cache,
) -> Vec<iced::widget::text::Span<'a, ()>> {
    let mut spans = Vec::new();
    let cursor = app.editor.cursor();
    // Explicit Cat/conventional selection must outrank the implicit at-point
    // Cat highlight; otherwise mouse dragging would remain visually hidden.
    let selection = app.editor.selection().or_else(|| app.cat_render_highlight());
    let extended = app.editor.cat_highlight().is_some() || app.editor.selection().is_some();

    for (index, region) in app.editor.regions().iter().enumerate() {
        if let Some(separator) = separator_for_region(app, index, columns) {
            if index > 0 { spans.push(span("\n")); }
            spans.push(span(separator).font(IOSEVKA).color(secondary_text(mode)));
            spans.push(span("\n\n"));
        } else if index > 0 {
            spans.push(span("\n\n"));
        }

        let value = region.text.as_str();
        let styles = cache.ranges(region.document, value);
        let selected = selection.and_then(|(start, end)| {
            if index < start.region || index > end.region { return None; }
            let first = if index == start.region { start.byte } else { 0 };
            let last = if index == end.region { end.byte } else { value.len() };
            (first < last).then_some((first, last))
        });
        let caret = (index == cursor.region).then_some(cursor.byte);
        let caret_end = caret.map(|byte| {
            value[byte..].chars().next().map_or(byte, |ch| byte + ch.len_utf8())
        });

        let mut boundaries = vec![0, value.len()];
        if let Some(byte) = caret { boundaries.push(byte); }
        if let Some(byte) = caret_end { boundaries.push(byte); }
        if let Some((start, end)) = selected { boundaries.extend([start, end]); }
        for range in styles {
            boundaries.extend([range.start, range.end]);
        }
        boundaries.sort_unstable();
        boundaries.dedup();

        for pair in boundaries.windows(2) {
            let (start, end) = (pair[0], pair[1]);
            if end <= start { continue; }
            let fragment = &value[start..end];
            let is_caret = caret == Some(start);
            let newline_caret = is_caret && fragment == "\n";
            let display = if newline_caret { " " } else { fragment };
            let mut styled = span(display).font(IOSEVKA);
            if is_caret {
                styled = styled.background(caret_color(mode)).color(caret_foreground(mode));
            } else if selected.is_some_and(|(first, last)| first < end && start < last) {
                styled = styled
                    .background(if extended { selection_background(mode) } else { cat_highlight_background(mode) })
                    .color(selection_foreground(mode));
            } else if let Some(syntax) = markdown::syntax_at(styles, start, end) {
                styled = styled.color(syntax_color(mode, syntax));
            }
            spans.push(styled);
            if newline_caret { spans.push(span("\n").font(IOSEVKA)); }
        }
        if caret == Some(value.len()) {
            spans.push(
                span(" ").font(IOSEVKA)
                    .background(caret_color(mode))
                    .color(caret_foreground(mode)),
            );
        }
    }
    if spans.is_empty() {
        spans.push(span(" ").font(IOSEVKA).background(caret_color(mode)).color(caret_foreground(mode)));
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
    format!(
        "{prefix}{}",
        "─".repeat(columns.saturating_sub(prefix.chars().count()))
    )
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
