use std::{
    io::{Stdout, stdout},
    process::Child,
    sync::mpsc::{self, Receiver},
    time::{Duration, Instant, SystemTime},
};

use crate::speedtest::{self, TestResult, Update};
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
    let lands_at = local_time(SystemTime::now() + duration);
    let mut frame_number = 0_u64;
    let mut net = NetCheck::new();

    loop {
        let elapsed = started.elapsed();
        let remaining = duration.saturating_sub(elapsed);
        terminal.terminal.draw(|frame| {
            if net.open {
                draw_netcheck(frame, remaining, frame_number, &net);
            } else {
                draw(frame, duration, elapsed, remaining, &lands_at, frame_number);
            }
        })?;

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
        {
            if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
                return Ok(());
            }
            match key.code {
                KeyCode::Char('q') => return Ok(()),
                KeyCode::Esc if net.open => net.close(),
                KeyCode::Esc => return Ok(()),
                KeyCode::Char('i') => net.toggle(),
                KeyCode::Char(' ') | KeyCode::Enter | KeyCode::Char('r')
                    if net.open && !net.running =>
                {
                    net.start();
                }
                _ => {}
            }
        }

        net.pump();
        frame_number = frame_number.wrapping_add(1);
    }
}

struct NetCheck {
    open: bool,
    running: bool,
    phase: &'static str,
    live_speed_mbps: f64,
    live_elapsed: Duration,
    result: Option<TestResult>,
    error: Option<String>,
    receiver: Option<Receiver<Update>>,
}

impl NetCheck {
    fn new() -> Self {
        Self {
            open: false,
            running: false,
            phase: "Connecting to mission control",
            live_speed_mbps: 0.0,
            live_elapsed: Duration::ZERO,
            result: None,
            error: None,
            receiver: None,
        }
    }

    fn toggle(&mut self) {
        self.open = !self.open;
        if self.open && !self.running && self.result.is_none() && self.error.is_none() {
            self.start();
        }
    }

    fn close(&mut self) {
        self.open = false;
    }

    fn start(&mut self) {
        let (sender, receiver) = mpsc::channel();
        self.receiver = Some(receiver);
        self.running = true;
        self.phase = "Connecting to mission control";
        self.live_speed_mbps = 0.0;
        self.live_elapsed = Duration::ZERO;
        self.result = None;
        self.error = None;
        std::thread::spawn(move || speedtest::run(sender));
    }

    fn pump(&mut self) {
        loop {
            let Some(receiver) = self.receiver.as_ref() else {
                return;
            };
            let update = match receiver.try_recv() {
                Ok(update) => update,
                Err(_) => return,
            };
            match update {
                Update::Phase(phase) => self.phase = phase,
                Update::Sample {
                    speed_mbps,
                    elapsed,
                } => {
                    self.live_speed_mbps = speed_mbps;
                    self.live_elapsed = elapsed;
                }
                Update::Finished(result) => {
                    self.result = Some(result);
                    self.running = false;
                    self.receiver = None;
                }
                Update::Failed(reason) => {
                    self.error = Some(reason);
                    self.running = false;
                    self.receiver = None;
                }
            }
        }
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
    lands_at: &str,
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
    draw_details(frame, area, duration, elapsed, lands_at);
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

    let keys = if area.width >= 40 {
        "i net · q quit"
    } else {
        "q quit"
    };
    let keys = Paragraph::new(keys)
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
    lands_at: &str,
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
    let details = vec![
        Line::styled(bar, Style::default().fg(Color::DarkGray)),
        Line::from(vec![
            Span::styled("caffeinate active", Style::default().fg(Color::Green)),
            Span::styled(
                format!("  ·  lands at {lands_at}"),
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

fn draw_netcheck(frame: &mut Frame, remaining: Duration, frame_number: u64, net: &NetCheck) {
    let area = frame.area();
    if area.width < 28 || area.height < 9 {
        draw_net_compact(frame, area, net, frame_number);
        return;
    }

    draw_stars(frame, area, frame_number);
    draw_net_header(frame, area);
    let content = Rect::new(
        area.x,
        area.y + 2,
        area.width,
        area.height.saturating_sub(4),
    );
    if net.running {
        draw_net_running(frame, content, net, frame_number);
    } else if let Some(result) = net.result.as_ref() {
        draw_net_report(frame, content, result);
    } else {
        draw_net_offline(frame, content, net, frame_number);
    }
    draw_net_footer(frame, area, remaining);
}

fn draw_net_header(frame: &mut Frame, area: Rect) {
    let header = Line::from(vec![
        Span::styled(
            " SLAP ",
            Style::default()
                .fg(Color::Black)
                .bg(Color::LightCyan)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled("  LINK CHECK", Style::default().fg(Color::DarkGray)),
    ]);
    frame.render_widget(
        Paragraph::new(header),
        Rect::new(area.x, area.y, area.width, 1),
    );

    let keys = if area.width >= 44 {
        "i back · space again · q quit"
    } else {
        "i · space · q"
    };
    let keys = Paragraph::new(keys)
        .style(Style::default().fg(Color::DarkGray))
        .alignment(Alignment::Right);
    frame.render_widget(
        keys,
        Rect::new(area.x, area.y, area.width.saturating_sub(1), 1),
    );
}

fn draw_net_footer(frame: &mut Frame, area: Rect, remaining: Duration) {
    let footer = Line::from(vec![
        Span::styled("caffeinate active", Style::default().fg(Color::Green)),
        Span::styled(
            format!("  ·  {} left", clock(remaining)),
            Style::default().fg(Color::DarkGray),
        ),
    ]);
    frame.render_widget(
        Paragraph::new(footer),
        Rect::new(
            area.x + 2,
            area.bottom().saturating_sub(1),
            area.width.saturating_sub(3),
            1,
        ),
    );
}

fn draw_net_running(frame: &mut Frame, area: Rect, net: &NetCheck, frame_number: u64) {
    let middle = area.y + area.height / 2;

    draw_centered(
        frame,
        Rect::new(area.x, middle.saturating_sub(2), area.width, 1),
        net.phase.to_owned(),
        Color::DarkGray,
        false,
    );

    let speed = Line::from(vec![
        Span::styled(
            format!("{:.1}", net.live_speed_mbps),
            Style::default()
                .fg(Color::LightCyan)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(" Mbps", Style::default().fg(Color::DarkGray)),
    ]);
    frame.render_widget(
        Paragraph::new(speed).alignment(Alignment::Center),
        Rect::new(area.x, middle.saturating_sub(1), area.width, 1),
    );

    draw_centered(
        frame,
        Rect::new(area.x, middle + 1, area.width, 1),
        format!("{}=[◇]▶", plume(frame_number)),
        Color::LightMagenta,
        true,
    );

    let bar_width = area.width.saturating_sub(8).min(34) as usize;
    let progress =
        (net.live_elapsed.as_secs_f64() / speedtest::TEST_DURATION.as_secs_f64()).clamp(0.0, 1.0);
    let filled = (bar_width as f64 * progress).round() as usize;
    let bar = format!(
        "{}{}",
        "━".repeat(filled),
        "─".repeat(bar_width.saturating_sub(filled))
    );
    draw_centered(
        frame,
        Rect::new(area.x, middle + 3, area.width, 1),
        bar,
        Color::DarkGray,
        false,
    );
}

fn draw_net_report(frame: &mut Frame, area: Rect, result: &TestResult) {
    let middle = area.y + area.height / 2;
    let megabytes = result.bytes as f64 / 1_000_000.0;
    let rows = big_speed_rows(result.speed_mbps);
    let digits_width = rows.first().map_or(0, |row| row.chars().count()) as u16;

    if area.width >= digits_width + 2 {
        for (offset, row) in rows.iter().enumerate() {
            draw_centered(
                frame,
                Rect::new(
                    area.x,
                    middle.saturating_sub(2) + offset as u16,
                    area.width,
                    1,
                ),
                row.clone(),
                Color::LightCyan,
                true,
            );
        }
        draw_centered(
            frame,
            Rect::new(area.x, middle + 2, area.width, 1),
            "DOWNLOAD · Mbps".to_owned(),
            Color::DarkGray,
            false,
        );
        let report = Line::from(vec![
            Span::styled(
                format!("{megabytes:.1} MB pulled"),
                Style::default()
                    .fg(Color::Green)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!("  ·  {} ms ping", result.latency_ms),
                Style::default().fg(Color::Gray),
            ),
        ]);
        frame.render_widget(
            Paragraph::new(report).alignment(Alignment::Center),
            Rect::new(area.x, middle + 3, area.width, 1),
        );
        if area.width >= 40 {
            draw_centered(
                frame,
                Rect::new(area.x, middle + 4, area.width, 1),
                result.server.clone(),
                Color::DarkGray,
                false,
            );
        }
        return;
    }

    draw_centered(
        frame,
        Rect::new(area.x, middle.saturating_sub(1), area.width, 1),
        format!("{:.1} Mbps · {:.1} MB", result.speed_mbps, megabytes),
        Color::LightCyan,
        true,
    );
    if area.width >= 40 {
        draw_centered(
            frame,
            Rect::new(area.x, middle, area.width, 1),
            format!("{} ms ping · {}", result.latency_ms, result.server),
            Color::Gray,
            false,
        );
    }
}

fn draw_net_offline(frame: &mut Frame, area: Rect, net: &NetCheck, frame_number: u64) {
    let middle = area.y + area.height / 2;

    draw_centered(
        frame,
        Rect::new(area.x, middle.saturating_sub(3), area.width, 1),
        format!("{}=[◇]▶", plume(frame_number)),
        Color::DarkGray,
        false,
    );
    draw_centered(
        frame,
        Rect::new(area.x, middle.saturating_sub(1), area.width, 1),
        "NO SIGNAL".to_owned(),
        Color::Yellow,
        true,
    );
    draw_centered(
        frame,
        Rect::new(area.x, middle, area.width, 1),
        "You appear to be offline — mission control is out of range.".to_owned(),
        Color::DarkGray,
        false,
    );
    if area.width >= 44
        && let Some(reason) = net.error.as_deref()
    {
        draw_centered(
            frame,
            Rect::new(area.x, middle + 1, area.width, 1),
            truncate(reason, area.width as usize / 2),
            Color::DarkGray,
            false,
        );
    }
    draw_centered(
        frame,
        Rect::new(area.x, middle + 3, area.width, 1),
        "press space to try again".to_owned(),
        Color::Gray,
        false,
    );
}

fn draw_net_compact(frame: &mut Frame, area: Rect, net: &NetCheck, frame_number: u64) {
    let ship = match frame_number % 4 {
        0 => "·=[◇]▶",
        1 => "≈=[◇]▶",
        2 => "≋=[◇]▶",
        _ => "~=[◇]▶",
    };
    let status = if net.running {
        if net.live_speed_mbps > 0.0 {
            format!("{:.1} Mbps · testing", net.live_speed_mbps)
        } else {
            net.phase.to_owned()
        }
    } else if let Some(result) = net.result.as_ref() {
        format!(
            "{:.1} Mbps · {:.1} MB",
            result.speed_mbps,
            result.bytes as f64 / 1_000_000.0
        )
    } else {
        "no signal · offline".to_owned()
    };
    let lines = vec![
        Line::styled(
            "SLAP · LINK CHECK",
            Style::default()
                .fg(Color::LightCyan)
                .add_modifier(Modifier::BOLD),
        ),
        Line::styled(
            status,
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        ),
        Line::styled(ship, Style::default().fg(Color::LightMagenta)),
        Line::styled("i back · space again", Style::default().fg(Color::DarkGray)),
    ];
    frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: true }), area);
}

fn big_speed_rows(speed: f64) -> Vec<String> {
    let value = format!("{speed:.1}");
    let mut rows = vec![String::new(), String::new(), String::new()];
    for (index, character) in value.chars().enumerate() {
        let glyph = speed_glyph(character);
        for (row, pattern) in rows.iter_mut().zip(glyph) {
            if index > 0 {
                row.push(' ');
            }
            row.push_str(pattern);
        }
    }
    rows
}

fn speed_glyph(character: char) -> [&'static str; 3] {
    match character {
        '0' => ["╭─╮", "│ │", "╰─╯"],
        '1' => [" ╷ ", " │ ", " ╵ "],
        '2' => ["╭─╮", "╭─╯", "╰─╴"],
        '3' => ["╭─╮", " ╶┤", "╰─╯"],
        '4' => ["╷ ╷", "╰─┤", "  ╵"],
        '5' => ["╭─╴", "╰─╮", "╰─╯"],
        '6' => ["╭─╴", "├─╮", "╰─╯"],
        '7' => ["╭─╮", "  │", "  ╵"],
        '8' => ["╭─╮", "├─┤", "╰─╯"],
        '9' => ["╭─╮", "╰─┤", "  ╵"],
        '.' => [" ", " ", "•"],
        _ => ["   ", "   ", "   "],
    }
}

fn draw_centered(frame: &mut Frame, area: Rect, text: String, color: Color, bold: bool) {
    let mut style = Style::default().fg(color);
    if bold {
        style = style.add_modifier(Modifier::BOLD);
    }
    frame.render_widget(
        Paragraph::new(text)
            .alignment(Alignment::Center)
            .style(style),
        area,
    );
}

fn truncate(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        text.to_owned()
    } else {
        let cut: String = text.chars().take(max.saturating_sub(1)).collect();
        format!("{cut}…")
    }
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

/// `time` as the clock shows it, such as `10:30 PM`, or "later" if it cannot be worked out. It
/// runs `date`, so it is worked out once per session, not on every frame.
fn local_time(time: SystemTime) -> String {
    let Ok(since_epoch) = time.duration_since(SystemTime::UNIX_EPOCH) else {
        return "later".to_owned();
    };
    std::process::Command::new("date")
        .args(["-r", &since_epoch.as_secs().to_string(), "+%l:%M %p"])
        .output()
        .ok()
        .filter(|output| output.status.success())
        .and_then(|output| String::from_utf8(output.stdout).ok())
        .map(|time| time.trim().to_owned())
        .unwrap_or_else(|| "later".to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::{Terminal, backend::TestBackend};

    fn render(net: &NetCheck, width: u16, height: u16) -> String {
        let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
        terminal
            .draw(|frame| draw_netcheck(frame, Duration::from_secs(300), 0, net))
            .unwrap();
        let buffer = terminal.backend().buffer().clone();
        buffer.content().iter().map(|cell| cell.symbol()).collect()
    }

    #[test]
    fn timer_shows_time_left_and_when_it_lands() {
        let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
        terminal
            .draw(|frame| {
                draw(
                    frame,
                    Duration::from_secs(300),
                    Duration::from_secs(60),
                    Duration::from_secs(240),
                    "10:30 PM",
                    0,
                );
            })
            .unwrap();
        let buffer = terminal.backend().buffer().clone();
        let text: String = buffer.content().iter().map(|cell| cell.symbol()).collect();
        assert!(text.contains("04:00"));
        assert!(text.contains("lands at 10:30 PM"));
    }

    #[test]
    fn local_time_reads_as_a_clock() {
        let time = local_time(SystemTime::now());
        assert!(time.contains(':') && time != "later", "{time}");
    }

    fn offline_check() -> NetCheck {
        let mut net = NetCheck::new();
        net.open = true;
        net.running = false;
        net.error = Some("could not reach Fast.com".to_owned());
        net
    }

    #[test]
    fn offline_state_is_graceful() {
        let text = render(&offline_check(), 80, 24);
        assert!(text.contains("NO SIGNAL"));
        assert!(text.contains("offline"));
        assert!(text.contains("press space to try again"));
    }

    #[test]
    fn offline_state_compact_stays_small() {
        let text = render(&offline_check(), 24, 8);
        assert!(text.contains("no signal"));
        assert!(text.contains("i back"));
    }

    #[test]
    fn report_shows_speed_and_megabytes() {
        let mut net = NetCheck::new();
        net.open = true;
        net.result = Some(TestResult {
            speed_mbps: 123.4,
            latency_ms: 21,
            bytes: 150_000_000,
            server: "Frankfurt, Germany".to_owned(),
        });
        let text = render(&net, 80, 24);
        assert!(text.contains("DOWNLOAD"));
        assert!(text.contains("MB pulled"));
        assert!(text.contains("21 ms ping"));
        assert!(text.contains("Frankfurt, Germany"));
    }

    #[test]
    fn report_compact_fits_narrow_terminal() {
        let mut net = NetCheck::new();
        net.open = true;
        net.result = Some(TestResult {
            speed_mbps: 12.5,
            latency_ms: 21,
            bytes: 15_000_000,
            server: "Frankfurt, Germany".to_owned(),
        });
        let text = render(&net, 24, 8);
        assert!(text.contains("12.5 Mbps"));
        assert!(text.contains("15.0 MB"));
    }

    #[test]
    fn running_state_shows_phase_and_gauge() {
        let mut net = NetCheck::new();
        net.open = true;
        net.running = true;
        net.phase = "Opening the taps";
        net.live_speed_mbps = 45.2;
        net.live_elapsed = Duration::from_secs(5);
        let text = render(&net, 80, 24);
        assert!(text.contains("Opening the taps"));
        assert!(text.contains("45.2"));
        assert!(text.contains("Mbps"));
        assert!(text.contains('━'));
        assert!(text.contains("caffeinate active"));
    }
}
