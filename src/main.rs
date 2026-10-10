mod speedtest;
mod tui;

use std::{
    io::IsTerminal,
    process::{Command, Stdio},
    time::Duration,
};

use anyhow::{Context, Result, bail};

const HELP: &str = concat!(
    "slap ",
    env!("CARGO_PKG_VERSION"),
    " — keep your Mac awake, beautifully\n\nUSAGE:\n    slap <duration>\n\nEXAMPLES:\n    slap 5-minutes\n    slap 1-hour\n    slap 5-hours\n    slap 30s\n    slap 15m\n    slap 2h\n\nKEYS:\n    i                   Run an internet speed check (i or Esc to return)\n    q / Esc / Ctrl-C    End the session\n",
);

fn main() -> Result<()> {
    let arguments: Vec<String> = std::env::args().skip(1).collect();

    match arguments.as_slice() {
        [flag] if matches!(flag.as_str(), "-h" | "--help") => {
            print!("{HELP}");
            return Ok(());
        }
        [flag] if matches!(flag.as_str(), "-V" | "--version") => {
            println!("slap {}", env!("CARGO_PKG_VERSION"));
            return Ok(());
        }
        [] => {
            eprintln!("Missing a duration.\n\n{HELP}");
            std::process::exit(2);
        }
        _ => {}
    }

    ensure_macos()?;

    let requested = arguments.join(" ");
    let duration =
        parse_duration(&requested).with_context(|| format!("Invalid duration: {requested}"))?;

    let mut caffeinate = Command::new("caffeinate")
        .args(["-dimsu", "-t", &duration.as_secs().to_string()])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .context("Could not start macOS caffeinate")?;

    let result = if std::io::stdout().is_terminal() {
        tui::run(duration, &mut caffeinate)
    } else {
        println!("Keeping this Mac awake for {}.", format_duration(duration));
        caffeinate
            .wait()
            .context("Failed while waiting for caffeinate")?;
        println!("Session complete.");
        Ok(())
    };

    if caffeinate.try_wait()?.is_none() {
        caffeinate.kill().context("Could not stop caffeinate")?;
        let _ = caffeinate.wait();
    }

    result
}

#[cfg(target_os = "macos")]
fn ensure_macos() -> Result<()> {
    Ok(())
}

#[cfg(not(target_os = "macos"))]
fn ensure_macos() -> Result<()> {
    bail!("slap is available only on macOS because it wraps caffeinate")
}

fn parse_duration(input: &str) -> Result<Duration> {
    let normalized = input.trim().to_ascii_lowercase().replace('_', "-");
    let compact: String = normalized
        .chars()
        .filter(|character| !character.is_whitespace())
        .collect();
    let compact = compact.trim_start_matches('-');

    let number_length = compact.chars().take_while(char::is_ascii_digit).count();
    if number_length == 0 {
        bail!("duration must start with a number")
    }

    let value: u64 = compact[..number_length]
        .parse()
        .context("duration is too large")?;
    if value == 0 {
        bail!("duration must be greater than zero")
    }

    let unit = compact[number_length..].trim_start_matches('-');
    let multiplier = match unit {
        "s" | "sec" | "secs" | "second" | "seconds" => 1,
        "m" | "min" | "mins" | "minute" | "minutes" => 60,
        "h" | "hr" | "hrs" | "hour" | "hours" => 60 * 60,
        _ => bail!("use seconds, minutes, or hours (for example: 30s, 5m, or 1-hour)"),
    };

    let seconds = value
        .checked_mul(multiplier)
        .context("duration is too large")?;
    if seconds > 365 * 24 * 60 * 60 {
        bail!("duration cannot be longer than one year")
    }

    Ok(Duration::from_secs(seconds))
}

fn format_duration(duration: Duration) -> String {
    let seconds = duration.as_secs();
    if seconds.is_multiple_of(3_600) {
        plural(seconds / 3_600, "hour")
    } else if seconds.is_multiple_of(60) {
        plural(seconds / 60, "minute")
    } else {
        plural(seconds, "second")
    }
}

fn plural(value: u64, unit: &str) -> String {
    let suffix = if value == 1 { "" } else { "s" };
    format!("{value} {unit}{suffix}")
}
