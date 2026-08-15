use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
    Frame,
};

use crate::state::{table::TableState, UiState};

const COLOR_YELLOW: Color = Color::Rgb(224, 175, 104);
const COLOR_CYAN: Color = Color::Rgb(125, 207, 255);
const COLOR_RED: Color = Color::Rgb(247, 118, 142);
const COLOR_GREEN: Color = Color::Rgb(158, 206, 106);
const COLOR_COMMENT: Color = Color::Rgb(86, 95, 137);
const COLOR_BG: Color = Color::Rgb(26, 27, 38); // Tokyo Night background

pub fn render_player_turn_popup(frame: &mut Frame, area: Rect, ui: &UiState) {
    let crate::state::Screen::Table(ref table) = ui.screen else {
        return;
    };
    if !table.is_my_turn {
        return;
    }

    let popup_area = centered_popup(52, 10, area);

    // Clear background under popup
    frame.render_widget(Clear, popup_area);

    let block = Block::default()
        .title(Line::from(vec![
            Span::styled(" ▶ ", Style::default().fg(COLOR_YELLOW)),
            Span::styled(
                "YOUR TURN",
                Style::default()
                    .fg(COLOR_YELLOW)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(" ", Style::default()),
        ]))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(COLOR_YELLOW))
        .style(Style::default().bg(COLOR_BG));

    let inner = block.inner(popup_area);
    frame.render_widget(block, popup_area);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // hand info
            Constraint::Length(1), // spacer
            Constraint::Length(1), // buttons
            Constraint::Min(0),
        ])
        .split(inner);

    // Hand value line
    let hand_line = build_hand_line(table);
    frame.render_widget(
        Paragraph::new(hand_line).alignment(Alignment::Center),
        chunks[0],
    );

    // Buttons
    let buttons = Line::from(vec![
        Span::styled(
            "[ H ] Hit",
            Style::default()
                .fg(COLOR_GREEN)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw("    "),
        Span::styled(
            "[ S ] Stand",
            Style::default().fg(COLOR_RED).add_modifier(Modifier::BOLD),
        ),
        Span::raw("    "),
        Span::styled(
            "[ D ] Double",
            Style::default().fg(COLOR_CYAN).add_modifier(Modifier::BOLD),
        ),
    ]);
    frame.render_widget(
        Paragraph::new(buttons).alignment(Alignment::Center),
        chunks[2],
    );
}

fn build_hand_line(table: &TableState) -> Line<'static> {
    let me = table.players.iter().find(|p| p.active);
    let Some(p) = me else {
        return Line::from(vec![Span::styled(
            "Your hand: —",
            Style::default().fg(COLOR_COMMENT),
        )]);
    };

    let cards_str = p
        .hand
        .cards
        .iter()
        .map(|c| c.short_display())
        .collect::<Vec<_>>()
        .join("  ");

    let value_str = if p.is_bust {
        "  BUST".to_string()
    } else if p.hand_value > 0 {
        format!("  = {}", p.hand_value)
    } else {
        String::new()
    };

    Line::from(vec![
        Span::styled("Hand: ", Style::default().fg(COLOR_COMMENT)),
        Span::styled(
            cards_str,
            Style::default().fg(COLOR_CYAN).add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            value_str,
            Style::default()
                .fg(COLOR_YELLOW)
                .add_modifier(Modifier::BOLD),
        ),
    ])
}

fn centered_popup(width: u16, height: u16, area: Rect) -> Rect {
    let x = area.x + area.width.saturating_sub(width) / 2;
    let y = area.y + area.height.saturating_sub(height) / 2;
    Rect::new(x, y, width.min(area.width), height.min(area.height))
}
