use carta_app::{App, AppMode, View};
use chrono::{DateTime, Local};
use iced::theme::Mode as ThemeMode;
use iced::widget::{
    button, column, container, mouse_area, rich_text, row, scrollable, space, span, stack, text,
    text_input, Id,
};
use iced::{color, Color, Element, Font, Length};
use std::time::Instant;

use crate::layout::{Layout, Row};
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
    if let Some(path) = &state.setup_path {
        let mut content = column![
            text("Carta Space").font(IOSEVKA).size(24),
            text("Nessun archivio nella posizione standard")
                .font(IOSEVKA)
                .size(16),
            text(path.display().to_string()).font(IOSEVKA).size(14),
            button("Crea nuovo archivio").on_press(Message::CreateDefaultArchive),
            text("Oppure clona un archivio Git esistente:").font(IOSEVKA),
            text_input("URL del repository Git", &state.clone_url)
                .on_input(Message::CloneUrlChanged)
                .on_submit(Message::CloneDefaultArchive),
            button("Clona archivio").on_press(Message::CloneDefaultArchive),
        ]
        .spacing(12)
        .padding(24);
        if let Some(error) = &state.error {
            content = content.push(text(error).font(IOSEVKA));
        }
        return container(content)
            .width(Length::Fill)
            .height(Length::Fill)
            .into();
    }
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
    let status = status_bar(app, state.theme_mode, state.sync_error.as_deref());

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

fn editor_view<'a>(
    state: &Gui,
    app: &'a App,
    mode: ThemeMode,
    window: iced::Size,
) -> Element<'a, Message> {
    let columns = viewport::columns(window.width, state.font_size);
    let line_height = viewport::line_height(state.font_size);
    let height = viewport::writing_area_height(window.height);
    let top_padding = height * (2.0 / 3.0);

    // Index all Document rows once, then lay out and shape only a bounded
    // window. Neither keystrokes nor cursor navigation rebuild the View text.
    let mut layout = state.layout.borrow_mut();
    layout.sync(app, columns);
    let (first, last) = layout.window(
        state.scroll_y,
        height,
        line_height,
        top_padding + viewport::VERTICAL_PADDING,
    );
    let spans = window_spans(
        app,
        mode,
        columns,
        first,
        last,
        &layout,
        &mut state.markdown.borrow_mut(),
    );
    let total_rows = layout.total_rows();
    drop(layout);

    let text_view = rich_text(spans)
        .font(IOSEVKA)
        .size(state.font_size)
        .line_height(iced::Pixels(line_height))
        .width(Length::Fill);

    let page = container(text_view)
        .width(Length::Fill)
        .height(Length::Fixed(
            (last.saturating_sub(first).max(1) as f32) * line_height,
        ))
        .max_width(EDITOR_MAX_WIDTH)
        .padding([0, viewport::HORIZONTAL_PADDING as u16]);

    // The pointer is relative to the visible window (not the entire Archive).
    let area = mouse_area(container(page).center_x(Length::Fill))
        .on_move(Message::PointerMoved)
        .on_press(Message::PointerPressed)
        .on_release(Message::PointerReleased)
        .interaction(iced::mouse::Interaction::Text);

    let leading = top_padding + viewport::VERTICAL_PADDING + first as f32 * line_height;
    let trailing = total_rows.saturating_sub(last) as f32 * line_height
        + viewport::VERTICAL_PADDING
        + top_padding;
    let sheet = column![space().height(leading), area, space().height(trailing)]
        .spacing(0)
        .width(Length::Fill);

    scrollable(sheet)
        .id(Id::new(EDITOR_SCROLL_ID))
        .on_scroll(|viewport| Message::Scrolled(viewport.absolute_offset().y))
        .height(Length::Fill)
        .width(Length::Fill)
        .into()
}

fn syntax_color(mode: ThemeMode, syntax: Syntax) -> Color {
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

/// Build a bounded set of spans. All strings refer to unchanged source
/// slices. Generated lines are presentation-only.
fn window_spans<'a>(
    app: &'a App,
    mode: ThemeMode,
    columns: usize,
    first: usize,
    last: usize,
    layout: &Layout,
    syntax_cache: &mut markdown::Cache,
) -> Vec<iced::widget::text::Span<'a, ()>> {
    let mut spans = Vec::new();
    let cursor = app.editor.cursor();
    let selection = app
        .editor
        .selection()
        .or_else(|| app.cat_render_highlight());
    let extended = app.editor.cat_highlight().is_some() || app.editor.selection().is_some();

    for row in first..last {
        if row > first {
            spans.push(span("\n").font(IOSEVKA));
        }
        match layout.row(app, row) {
            Some(Row::Gap) => {}
            Some(Row::Rule { region }) => {
                if let Some(separator) = separator_for_region(app, region, columns) {
                    spans.push(span(separator).font(IOSEVKA).color(secondary_text(mode)));
                }
            }
            Some(Row::Text {
                region,
                start,
                end,
                hard_break,
            }) => {
                let item = &app.editor.regions()[region];
                let text = &item.text;
                let syntax = syntax_cache.ranges(item.document, text, layout.generation(region));
                let selected = selection.and_then(|(a, b)| {
                    if region < a.region || region > b.region {
                        return None;
                    }
                    let first_byte = if region == a.region { a.byte } else { 0 };
                    let last_byte = if region == b.region {
                        b.byte
                    } else {
                        text.len()
                    };
                    (first_byte < last_byte).then_some((first_byte, last_byte))
                });
                let is_cursor_region = region == cursor.region;
                let cursor_here = is_cursor_region && cursor.byte >= start && cursor.byte < end;
                let caret_end = if cursor_here {
                    text[cursor.byte..]
                        .chars()
                        .next()
                        .map_or(cursor.byte, |ch| cursor.byte + ch.len_utf8())
                } else {
                    cursor.byte
                };

                let mut boundaries = vec![start, end];
                if cursor_here {
                    boundaries.extend([cursor.byte, caret_end]);
                }
                if let Some((a, b)) = selected {
                    boundaries.extend([a.clamp(start, end), b.clamp(start, end)]);
                }
                let first_style = syntax.partition_point(|range| range.end <= start);
                for range in syntax[first_style..]
                    .iter()
                    .take_while(|range| range.start < end)
                {
                    boundaries.push(range.start.clamp(start, end));
                    boundaries.push(range.end.clamp(start, end));
                }
                boundaries.sort_unstable();
                boundaries.dedup();

                for pair in boundaries.windows(2) {
                    let (a, b) = (pair[0], pair[1]);
                    if a == b {
                        continue;
                    }
                    let mut styled = span(&text[a..b]).font(IOSEVKA);
                    if cursor_here && a == cursor.byte {
                        styled = styled
                            .background(caret_color(mode))
                            .color(caret_foreground(mode));
                    } else if selected.is_some_and(|(from, to)| from < b && a < to) {
                        styled = styled
                            .background(if extended {
                                selection_background(mode)
                            } else {
                                cat_highlight_background(mode)
                            })
                            .color(selection_foreground(mode));
                    } else if let Some(syntax) = markdown::style_at(syntax, a) {
                        styled = styled.color(syntax_color(mode, syntax));
                    }
                    spans.push(styled);
                }
                if is_cursor_region && cursor.byte == end && (hard_break || end == text.len()) {
                    spans.push(
                        span(" ")
                            .font(IOSEVKA)
                            .background(caret_color(mode))
                            .color(caret_foreground(mode)),
                    );
                }
            }
            None => {}
        }
    }
    if spans.is_empty() {
        spans.push(span(" ").font(IOSEVKA));
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

fn status_bar<'a>(app: &App, mode: ThemeMode, sync_error: Option<&'a str>) -> Element<'a, Message> {
    let failed = sync_error.is_some();
    let status = app.status_bar();
    let (background, foreground) = if failed {
        (sync_error_background(mode), color!(0xFFFFFF))
    } else {
        status_palette(app, mode)
    };
    let right = text(status.right).font(IOSEVKA).size(STATUS_SIZE);
    let left = if let Some(error) = sync_error {
        // Keep the error persistent, independently of expiring App statuses.
        let shortened: String = error.chars().take(140).collect();
        format!("SYNC FALLITA · {shortened} · ritento automatico")
    } else {
        status.left
    };
    let line = row![
        text(left)
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
                .background(background)
                .color(foreground)
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

fn status_palette(app: &App, mode: ThemeMode) -> (Color, Color) {
    match &app.view {
        View::ModificationDate => {
            let background = color!(0x315EAB);
            (background, readable_status_text(background))
        }
        View::Work(id) => {
            let background = app
                .archive
                .work(*id)
                .and_then(|work| work.color())
                .and_then(parse_status_hex)
                .unwrap_or_else(|| status_background(mode));
            (background, readable_status_text(background))
        }
        _ => (status_background(mode), status_foreground(mode)),
    }
}

fn parse_status_hex(value: &str) -> Option<Color> {
    let hex = value.strip_prefix('#')?;
    if hex.len() != 6 {
        return None;
    }
    let red = u8::from_str_radix(&hex[..2], 16).ok()?;
    let green = u8::from_str_radix(&hex[2..4], 16).ok()?;
    let blue = u8::from_str_radix(&hex[4..6], 16).ok()?;
    Some(Color {
        r: f32::from(red) / 255.0,
        g: f32::from(green) / 255.0,
        b: f32::from(blue) / 255.0,
        a: 1.0,
    })
}

fn status_luminance(color: Color) -> f32 {
    fn linear(value: f32) -> f32 {
        if value <= 0.04045 {
            value / 12.92
        } else {
            ((value + 0.055) / 1.055).powf(2.4)
        }
    }
    0.2126 * linear(color.r) + 0.7152 * linear(color.g) + 0.0722 * linear(color.b)
}

fn readable_status_text(background: Color) -> Color {
    if status_luminance(background) > 0.179 {
        color!(0x000000)
    } else {
        color!(0xFFFFFF)
    }
}

fn sync_error_background(mode: ThemeMode) -> Color {
    match mode {
        ThemeMode::Light => color!(0xB42318),
        ThemeMode::Dark | ThemeMode::None => color!(0x991B1B),
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
    fn failed_remote_sync_has_red_status_background_in_both_themes() {
        for mode in [ThemeMode::Light, ThemeMode::Dark] {
            let red = sync_error_background(mode);
            assert!(red.r > red.g * 2.0);
            assert!(red.r > red.b * 1.5);
            assert_ne!(red, status_background(mode));
        }
    }

    #[test]
    fn work_accent_foreground_meets_wcag_aa_contrast() {
        for hex in ["#236B61", "#3F5794", "#B5B9A4", "#FFFFFF", "#000000"] {
            let background = parse_status_hex(hex).expect("valid stored accent");
            let foreground = readable_status_text(background);
            let luminance = status_luminance(background);
            let contrast = if foreground == color!(0x000000) {
                (luminance + 0.05) / 0.05
            } else {
                1.05 / (luminance + 0.05)
            };
            assert!(contrast >= 4.5, "{hex} contrast {contrast}");
        }
        assert!(parse_status_hex("#xyzxyz").is_none());
        assert_ne!(color!(0x315EAB), status_background(ThemeMode::Dark));
    }

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
