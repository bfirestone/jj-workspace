//! Ratatui rendering for the jw workspace picker.

use ansi_to_tui::IntoText;
use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Wrap};

use crate::app::{App, Mode};

/// Render the whole screen for the current app state.
pub fn render(frame: &mut Frame, app: &App) {
    // --- Top-level vertical layout ---
    let outer = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // filter line
            Constraint::Min(0),    // body
            Constraint::Length(1), // footer
        ])
        .split(frame.area());

    // --- Filter line ---
    let t = app.theme();
    let filter_line = Paragraph::new(Line::from(vec![
        Span::styled("> ", Style::default().fg(t.accent)),
        Span::raw(app.filter()),
    ]));
    frame.render_widget(filter_line, outer[0]);

    // --- Body: list | preview ---
    let body = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(45), Constraint::Percentage(55)])
        .split(outer[1]);

    render_list(frame, app, body[0]);
    render_preview(frame, app, body[1]);

    // --- Footer ---
    render_footer(frame, app, outer[2]);

    // --- Modal overlays ---
    match app.mode() {
        Mode::NewName => render_new_name_overlay(frame, app),
        Mode::ConfirmForget => render_confirm_forget_overlay(frame, app),
        Mode::Normal => {}
    }
}

fn render_list(frame: &mut Frame, app: &App, area: Rect) {
    let t = app.theme();
    let selected_name = app.selected_workspace().map(|w| w.name.as_str().to_owned());

    let mut lines: Vec<Line> = Vec::new();
    for (ws, positions) in app.visible_matches() {
        let is_selected = selected_name.as_deref() == Some(ws.name.as_str());

        // Marker
        let marker = if is_selected {
            Span::styled("▸ ", Style::default().fg(t.marker))
        } else {
            Span::raw("  ")
        };

        // Build name with highlighted characters. `positions` is a short slice of
        // match indices, so a direct `contains` check beats allocating a HashSet.
        let mut name_spans: Vec<Span> = Vec::new();
        let name_chars: Vec<char> = ws.name.chars().collect();

        let base_style = if ws.is_current {
            Style::default().add_modifier(Modifier::BOLD)
        } else if is_selected {
            Style::default().fg(t.selected)
        } else {
            Style::default().fg(t.normal)
        };

        let highlight_style = if ws.is_current {
            Style::default().add_modifier(Modifier::BOLD).fg(t.accent)
        } else {
            Style::default().fg(t.accent)
        };

        for (i, ch) in name_chars.iter().enumerate() {
            let style = if positions.contains(&i) {
                highlight_style
            } else {
                base_style
            };
            name_spans.push(Span::styled(ch.to_string(), style));
        }

        // Flags
        let mut flags = String::new();
        if ws.is_current {
            flags.push_str(" *");
        }
        if ws.conflict {
            flags.push_str(" !");
        }
        if ws.empty {
            flags.push_str(" ø");
        }
        if ws.stale {
            flags.push_str(" ~");
        }

        // Dim path
        let path_str = format!("  {}", ws.path.display());

        let mut spans: Vec<Span> = vec![marker];
        spans.extend(name_spans);
        if !flags.is_empty() {
            spans.push(Span::styled(flags, Style::default().fg(t.dim)));
        }
        spans.push(Span::styled(
            path_str,
            Style::default().fg(t.dim).add_modifier(Modifier::DIM),
        ));

        let mut line = Line::from(spans);
        if is_selected {
            line = line.style(Style::default().bg(t.selection_bg));
        }

        lines.push(line);
    }

    let list_block = Block::default().borders(Borders::NONE);
    let list_para = Paragraph::new(Text::from(lines)).block(list_block);
    frame.render_widget(list_para, area);
}

fn render_preview(frame: &mut Frame, app: &App, area: Rect) {
    let t = app.theme();
    let block = Block::default()
        .title(" preview ")
        .borders(Borders::LEFT)
        .border_style(Style::default().fg(t.dim));

    let inner = block.inner(area);
    frame.render_widget(block, area);

    if let Some(ws) = app.selected_workspace() {
        // Header line: name@ change_id · description
        let mut header_parts = vec![
            Span::styled(
                format!("{}@", ws.name),
                Style::default().add_modifier(Modifier::BOLD),
            ),
            Span::raw(format!(" {} · {}", ws.change_id, ws.description)),
        ];
        if ws.conflict {
            header_parts.push(Span::styled(" [conflict]", Style::default().fg(t.conflict)));
        }
        if ws.empty {
            header_parts.push(Span::styled(" [empty]", Style::default().fg(t.dim)));
        }
        if ws.stale {
            header_parts.push(Span::styled(" [stale]", Style::default().fg(t.stale)));
        }

        let header_line = Line::from(header_parts);

        // Preview body
        let body_text: Text = match app.cached_preview(&ws.name) {
            Some(preview) => preview
                .as_bytes()
                .into_text()
                .unwrap_or_else(|_| Text::raw(preview)),
            None => Text::raw("loading…"),
        };

        // Combine header + body
        let mut lines: Vec<Line> = vec![header_line, Line::raw("")];
        lines.extend(body_text.lines);

        let preview_para = Paragraph::new(Text::from(lines));
        frame.render_widget(preview_para, inner);
    } else {
        let empty = Paragraph::new(Text::raw("(no selection)"));
        frame.render_widget(empty, inner);
    }
}

fn render_footer(frame: &mut Frame, app: &App, area: Rect) {
    let t = app.theme();
    let keys = "[enter] cd · [M-o] edit · [M-a] agent · [M-n] new · [M-d] remove · [esc] quit";
    let counts = format!(" {}/{}", app.filtered_count(), app.total_count());

    let footer = Line::from(vec![
        Span::styled(keys, Style::default().fg(t.dim)),
        Span::styled(counts, Style::default().fg(t.normal)),
    ]);

    frame.render_widget(Paragraph::new(footer), area);
}

/// Center a `Rect` of the given size within a larger `Rect`.
fn centered_rect(width: u16, height: u16, area: Rect) -> Rect {
    let x = area.x + area.width.saturating_sub(width) / 2;
    let y = area.y + area.height.saturating_sub(height) / 2;
    Rect {
        x,
        y,
        width: width.min(area.width),
        height: height.min(area.height),
    }
}

fn render_new_name_overlay(frame: &mut Frame, app: &App) {
    let t = app.theme();
    let area = centered_rect(40, 5, frame.area());
    frame.render_widget(Clear, area);

    let block = Block::default()
        .title(" New workspace name ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(t.marker));

    let inner = block.inner(area);
    frame.render_widget(block, area);

    let text = Text::from(vec![
        Line::from(vec![
            Span::styled("> ", Style::default().fg(t.accent)),
            Span::raw(app.input()),
        ]),
        Line::from(Span::styled(
            "letters, digits, . _ - / only",
            Style::default().fg(t.dim),
        )),
    ]);
    frame.render_widget(Paragraph::new(text), inner);
}

fn render_confirm_forget_overlay(frame: &mut Frame, app: &App) {
    let t = app.theme();
    let area = centered_rect(44, 5, frame.area());
    frame.render_widget(Clear, area);

    let block = Block::default()
        .title(" Confirm ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(t.conflict));

    let inner = block.inner(area);
    frame.render_widget(block, area);

    let name = app
        .selected_workspace()
        .map(|w| w.name.as_str())
        .unwrap_or("?");
    let text = Text::from(vec![
        Line::from(format!("forget '{name}' and delete its directory?")),
        Line::from(vec![
            Span::raw("press "),
            Span::styled("y", Style::default().fg(t.conflict).add_modifier(Modifier::BOLD)),
            Span::raw(" to confirm, "),
            Span::styled("N", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(" / esc to cancel"),
        ]),
    ]);
    frame.render_widget(Paragraph::new(text).wrap(Wrap { trim: false }), inner);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::App;
    use crate::config::Config;
    use crate::jj::Workspace;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    use ratatui::style::Color;
    use std::path::PathBuf;

    fn ws(name: &str) -> Workspace {
        Workspace {
            name: name.into(),
            path: PathBuf::from(format!("/repo.{name}")),
            change_id: "3f2a9c1c".into(),
            description: "wire up oauth".into(),
            conflict: false,
            empty: false,
            stale: false,
            is_current: name == "default",
        }
    }

    #[test]
    fn renders_without_panicking_and_shows_a_name() {
        let app = App::new(
            vec![ws("auth"), ws("default")],
            PathBuf::from("/work/repo"),
            Config::default(),
        );
        let mut term = Terminal::new(TestBackend::new(90, 20)).unwrap();
        term.draw(|f| render(f, &app)).unwrap();
        let buf = term.backend().buffer().clone();
        let text: String = buf.content().iter().map(|c| c.symbol()).collect();
        assert!(text.contains("auth"), "expected 'auth' in rendered output");
        assert!(text.contains("enter"), "expected 'enter' in footer");
    }

    #[test]
    fn theme_recolors_the_prompt() {
        let mut cfg = Config::default();
        cfg.theme.accent = Color::Magenta;
        let app = App::new(vec![ws("auth")], PathBuf::from("/work/repo"), cfg);
        let mut term = Terminal::new(TestBackend::new(90, 20)).unwrap();
        term.draw(|f| render(f, &app)).unwrap();
        let buf = term.backend().buffer().clone();
        // The filter prompt "> " sits at row 0, col 0; its fg should follow `accent`.
        let cell = buf.cell((0u16, 0u16)).unwrap();
        assert_eq!(
            cell.fg,
            Color::Magenta,
            "accent theme override should recolor the prompt"
        );
    }

    #[test]
    fn default_theme_keeps_legacy_prompt_color() {
        let app = App::new(
            vec![ws("auth")],
            PathBuf::from("/work/repo"),
            Config::default(),
        );
        let mut term = Terminal::new(TestBackend::new(90, 20)).unwrap();
        term.draw(|f| render(f, &app)).unwrap();
        let buf = term.backend().buffer().clone();
        let cell = buf.cell((0u16, 0u16)).unwrap();
        assert_eq!(cell.fg, Color::Yellow, "default prompt must stay Yellow");
    }

    #[test]
    fn renders_filter_text() {
        use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

        let mut app = App::new(
            vec![ws("auth"), ws("default")],
            PathBuf::from("/work/repo"),
            Config::default(),
        );
        app.on_key(KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE));

        let mut term = Terminal::new(TestBackend::new(90, 20)).unwrap();
        term.draw(|f| render(f, &app)).unwrap();
        let buf = term.backend().buffer().clone();
        let text: String = buf.content().iter().map(|c| c.symbol()).collect();
        assert!(
            text.contains('a'),
            "filter char 'a' should appear in rendered output"
        );
    }

    #[test]
    fn renders_counts_in_footer() {
        let app = App::new(
            vec![ws("auth"), ws("default")],
            PathBuf::from("/work/repo"),
            Config::default(),
        );
        let mut term = Terminal::new(TestBackend::new(90, 20)).unwrap();
        term.draw(|f| render(f, &app)).unwrap();
        let buf = term.backend().buffer().clone();
        let text: String = buf.content().iter().map(|c| c.symbol()).collect();
        // Footer shows "2/2" (filtered/total)
        assert!(
            text.contains("2/2"),
            "expected count '2/2' in footer, got: {}",
            &text[..100.min(text.len())]
        );
    }

    #[test]
    fn renders_new_name_overlay_in_newname_mode() {
        use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

        let mut app = App::new(
            vec![ws("auth"), ws("default")],
            PathBuf::from("/work/repo"),
            Config::default(),
        );
        // Enter NewName mode
        app.on_key(KeyEvent::new(KeyCode::Char('n'), KeyModifiers::ALT));

        let mut term = Terminal::new(TestBackend::new(90, 20)).unwrap();
        term.draw(|f| render(f, &app)).unwrap();
        let buf = term.backend().buffer().clone();
        let text: String = buf.content().iter().map(|c| c.symbol()).collect();
        assert!(
            text.contains("New workspace"),
            "expected 'New workspace' overlay in NewName mode"
        );
    }

    #[test]
    fn renders_confirm_forget_overlay_in_confirm_mode() {
        use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

        let mut app = App::new(
            vec![ws("auth"), ws("default")],
            PathBuf::from("/work/repo"),
            Config::default(),
        );
        // auth is selected (index 0); trigger ConfirmForget via M-d
        app.on_key(KeyEvent::new(KeyCode::Char('d'), KeyModifiers::ALT));

        let mut term = Terminal::new(TestBackend::new(90, 20)).unwrap();
        term.draw(|f| render(f, &app)).unwrap();
        let buf = term.backend().buffer().clone();
        let text: String = buf.content().iter().map(|c| c.symbol()).collect();
        assert!(
            text.contains("forget"),
            "expected 'forget' overlay in ConfirmForget mode"
        );
        // Regression: the y/N hint must be fully rendered, not clipped off the
        // right edge of the overlay box.
        assert!(
            text.contains("to confirm") && text.contains("to cancel"),
            "expected the y/N confirm hint to be visible in the overlay"
        );
    }

    #[test]
    fn renders_loading_when_no_preview_cached() {
        let app = App::new(
            vec![ws("auth"), ws("default")],
            PathBuf::from("/work/repo"),
            Config::default(),
        );
        let mut term = Terminal::new(TestBackend::new(90, 20)).unwrap();
        term.draw(|f| render(f, &app)).unwrap();
        let buf = term.backend().buffer().clone();
        let text: String = buf.content().iter().map(|c| c.symbol()).collect();
        assert!(
            text.contains("loading"),
            "expected 'loading…' in preview when no cache"
        );
    }

    #[test]
    fn renders_cached_preview() {
        let mut app = App::new(
            vec![ws("auth"), ws("default")],
            PathBuf::from("/work/repo"),
            Config::default(),
        );
        app.cache_preview("auth", "1 file changed".to_string());

        let mut term = Terminal::new(TestBackend::new(90, 20)).unwrap();
        term.draw(|f| render(f, &app)).unwrap();
        let buf = term.backend().buffer().clone();
        let text: String = buf.content().iter().map(|c| c.symbol()).collect();
        assert!(
            text.contains("file changed"),
            "expected cached preview text to appear"
        );
    }
}
