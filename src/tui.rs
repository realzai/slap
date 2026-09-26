use std::{
    io::{Stdout, stdout},
    process::Child,
    time::{Duration, Instant, SystemTime},
};

use anyhow::{Context, Result};
use crossterm::{
    event::{self, Event, KeyCode, KeyEventKind, KeyModifiers},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{
    Frame, Terminal,
    backend::CrosstermBackend,
    layout::{Alignment, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Paragraph, Wrap},
};

const TICK_RATE: Duration = Duration::from_millis(80);

pub fn run(duration: Duration, caffeinate: &mut Child) -> Result<()> {
    let mut terminal = TerminalGuard::new()?;
    let started = Instant::now();
    let finish_at = SystemTime::now() + duration;
    let mut frame_number = 0_u64;

    loop {
        let elapsed = started.elapsed();
        let remaining = duration.saturating_sub(elapsed);
        terminal
            .terminal
            .draw(|frame| draw(frame, duration, elapsed, remaining, finish_at, frame_number))?;

        if remaining.is_zero() || caffeinate.try_wait()?.is_some() {
            terminal
                .terminal
                .draw(|frame| draw_complete(frame, duration))?;
            std::thread::sleep(Duration::from_millis(900));
            return Ok(());
        }

        if event::poll(TICK_RATE)?
            && let Event::Key(key) = event::read()?
            && key.kind == KeyEventKind::Press
            && (matches!(key.code, KeyCode::Char('q') | KeyCode::Esc)
                || key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL))
        {
            return Ok(());
        }

        frame_number = frame_number.wrapping_add(1);
    }
}

struct TerminalGuard {
    terminal: Terminal<CrosstermBackend<Stdout>>,
}

impl TerminalGuard {
    fn new() -> Result<Self> {
        enable_raw_mode().context("Could not enter raw terminal mode")?;
        let mut output = stdout();
        execute!(output, EnterAlternateScreen).context("Could not open alternate screen")?;
        let mut terminal = Terminal::new(CrosstermBackend::new(output))?;
        terminal.hide_cursor()?;
        Ok(Self { terminal })
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        let _ = execute!(self.terminal.backend_mut(), LeaveAlternateScreen);
        let _ = self.terminal.show_cursor();
    }
}

fn draw(
    frame: &mut Frame,
    duration: Duration,
    elapsed: Duration,
    remaining: Duration,
    finish_at: SystemTime,
    frame_number: u64,
) {
    let area = frame.area();
    if area.width < 28 || area.height < 9 {
        draw_compact(frame, area, remaining, frame_number);
        return;
    }

    draw_stars(frame, area, frame_number);
    draw_header(frame, area);
    draw_orbit(frame, area, frame_number);
    draw_timer(frame, area, remaining);
    draw_details(frame, area, duration, elapsed, finish_at);
}

fn draw_header(frame: &mut Frame, area: Rect) {
    let header = Line::from(vec![
        Span::styled(
            " SLAP ",
            Style::default()
                .fg(Color::Black)
                .bg(Color::LightCyan)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled("  MAC AWAKE SESSION", Style::default().fg(Color::DarkGray)),
    ]);
    frame.render_widget(
        Paragraph::new(header),
        Rect::new(area.x, area.y, area.width, 1),
    );

    let keys = Paragraph::new("q quit")
        .style(Style::default().fg(Color::DarkGray))
        .alignment(Alignment::Right);
    frame.render_widget(
        keys,
        Rect::new(area.x, area.y, area.width.saturating_sub(1), 1),
    );
}

fn draw_orbit(frame: &mut Frame, area: Rect, frame_number: u64) {
    let center_x = area.width as f64 * 0.72;
    let center_y = area.height as f64 * 0.45;
    let radius_x = (area.width as f64 * 0.19).clamp(7.0, 22.0);
    let radius_y = (area.height as f64 * 0.25).clamp(2.0, 6.0);
    let angle = frame_number as f64 * 0.075;
    let ship_x = center_x + angle.cos() * radius_x;
    let ship_y = center_y + angle.sin() * radius_y;
    let moving_right = -angle.sin() >= 0.0;

    let planet = if area.height >= 15 {
        vec![
            "     .-\"\"-.     ",
            "  .-'  .::. '-.  ",
            " /   .::::::   \\ ",
            "|    ::::::     |",
            " \\   '::::'   / ",
            "  '-.______.-'  ",
        ]
    } else {
        vec!["  .-\"-.  ", " / .::. \\ ", " \\_____/ "]
    };
    let planet_width = planet.first().map_or(0, |line| line.chars().count()) as u16;
    let planet_height = planet.len() as u16;
    let planet_x = center_x.round() as u16;
    let planet_x = planet_x.saturating_sub(planet_width / 2);
    let planet_y = (center_y.round() as u16).saturating_sub(planet_height / 2);
    let planet_text = planet
        .into_iter()
        .map(|line| Line::styled(line, Style::default().fg(Color::Blue)))
        .collect::<Vec<_>>();
    frame.render_widget(
        Paragraph::new(planet_text),
        clipped(area, planet_x, planet_y, planet_width, planet_height),
    );

    let (ship, color) = if moving_right {
        (format!("{}=[◇]▶", plume(frame_number)), Color::LightCyan)
    } else {
        (format!("◀[◇]={}", plume(frame_number)), Color::LightMagenta)
    };
    let ship_width = ship.chars().count() as u16;
    let ship_x = (ship_x.round().max(0.0) as u16).saturating_sub(ship_width / 2);
    let ship_y = ship_y.round().max(1.0) as u16;
    frame.render_widget(
        Paragraph::new(ship).style(Style::default().fg(color).add_modifier(Modifier::BOLD)),
        clipped(area, ship_x, ship_y, ship_width, 1),
    );
}

fn plume(frame_number: u64) -> &'static str {
    match frame_number % 4 {
        0 => "·",
        1 => "≈",
        2 => "≋",
        _ => "~",
    }
}

fn draw_timer(frame: &mut Frame, area: Rect, remaining: Duration) {
    let timer = clock(remaining);
    let timer_y = area.height.saturating_sub(6).max(2);
    let width = area.width.min(36);
    let label = Paragraph::new(Line::from(vec![
        Span::styled("AWAKE  ", Style::default().fg(Color::DarkGray)),
        Span::styled(
            timer,
            Style::default()
                .fg(Color::LightCyan)
                .add_modifier(Modifier::BOLD),
        ),
    ]));
    frame.render_widget(
        label,
        Rect::new(area.x + 2, timer_y, width.saturating_sub(2), 1),
    );
}

fn draw_details(
    frame: &mut Frame,
    area: Rect,
    duration: Duration,
    elapsed: Duration,
    finish_at: SystemTime,
) {
    let y = area.height.saturating_sub(3);
    let progress = if duration.is_zero() {
        1.0
    } else {
        elapsed.as_secs_f64() / duration.as_secs_f64()
    };
    let bar_width = area.width.saturating_sub(6).min(42) as usize;
    let filled = (bar_width as f64 * progress.clamp(0.0, 1.0)).round() as usize;
    let bar = format!(
        "{}{}",
        "━".repeat(filled),
        "─".repeat(bar_width.saturating_sub(filled))
    );
    let end = finish_at
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|duration| local_time(duration.as_secs()))
        .unwrap_or_else(|_| "later".to_owned());
    let details = vec![
        Line::styled(bar, Style::default().fg(Color::DarkGray)),
        Line::from(vec![
            Span::styled("caffeinate active", Style::default().fg(Color::Green)),
            Span::styled(
                format!("  ·  lands at {end}"),
                Style::default().fg(Color::DarkGray),
            ),
        ]),
    ];
    frame.render_widget(
        Paragraph::new(details),
        Rect::new(area.x + 2, y, area.width.saturating_sub(3), 2),
    );
}

fn draw_compact(frame: &mut Frame, area: Rect, remaining: Duration, frame_number: u64) {
    let ship = match frame_number % 4 {
        0 => "·=[◇]▶",
        1 => "≈=[◇]▶",
        2 => "≋=[◇]▶",
        _ => "~=[◇]▶",
    };
    let lines = vec![
        Line::styled(
            "SLAP · AWAKE",
            Style::default()
                .fg(Color::LightCyan)
                .add_modifier(Modifier::BOLD),
        ),
        Line::styled(
            clock(remaining),
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        ),
        Line::styled(ship, Style::default().fg(Color::LightMagenta)),
        Line::styled("q quit", Style::default().fg(Color::DarkGray)),
    ];
    frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: true }), area);
}

fn draw_complete(frame: &mut Frame, duration: Duration) {
    let area = frame.area();
    let message = vec![
        Line::styled("       _..._", Style::default().fg(Color::Blue)),
        Line::styled("   ___/_[◇]_\\___", Style::default().fg(Color::LightCyan)),
        Line::styled(
            "MISSION COMPLETE",
            Style::default()
                .fg(Color::Green)
                .add_modifier(Modifier::BOLD),
        ),
        Line::styled(
            format!("Mac stayed awake for {}", super::format_duration(duration)),
            Style::default().fg(Color::Gray),
        ),
    ];
    frame.render_widget(Paragraph::new(message).alignment(Alignment::Center), area);
}

fn draw_stars(frame: &mut Frame, area: Rect, frame_number: u64) {
    if area.width < 2 || area.height < 2 {
        return;
    }
    for y in 1..area.height.saturating_sub(1) {
        for x in 0..area.width {
            let drift = frame_number / (4 + ((x as u64 + y as u64) % 5));
            let hash = (x as u64).wrapping_mul(73_856_093)
                ^ (y as u64).wrapping_mul(19_349_663)
                ^ drift.wrapping_mul(83_492_791);
            if hash.is_multiple_of(137) {
                let star = if hash.is_multiple_of(3) { "✦" } else { "·" };
                let color = if hash.is_multiple_of(5) {
                    Color::DarkGray
                } else {
                    Color::Gray
                };
                frame.render_widget(
                    Paragraph::new(star).style(Style::default().fg(color)),
                    Rect::new(area.x + x, area.y + y, 1, 1),
                );
            }
        }
    }
}

fn clipped(area: Rect, x: u16, y: u16, width: u16, height: u16) -> Rect {
    let x = x.min(area.right());
    let y = y.min(area.bottom());
    Rect::new(
        area.x + x.min(area.width),
        area.y + y.min(area.height),
        width.min(area.width.saturating_sub(x)),
        height.min(area.height.saturating_sub(y)),
    )
}

fn clock(duration: Duration) -> String {
    let total = duration.as_secs();
    let hours = total / 3_600;
    let minutes = total % 3_600 / 60;
    let seconds = total % 60;
    if hours > 0 {
        format!("{hours:02}:{minutes:02}:{seconds:02}")
    } else {
        format!("{minutes:02}:{seconds:02}")
    }
}

fn local_time(unix_seconds: u64) -> String {
    let output = std::process::Command::new("date")
        .args(["-r", &unix_seconds.to_string(), "+%l:%M %p"])
        .output();
    output
        .ok()
        .filter(|output| output.status.success())
        .and_then(|output| String::from_utf8(output.stdout).ok())
        .map(|time| time.trim().to_owned())
        .unwrap_or_else(|| "later".to_owned())
}
