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

    // `-w` ties the assertion to slap itself, so a slap that is killed does not leave the Mac
    // awake for the rest of the duration.
    let mut caffeinate = Command::new("caffeinate")
        .args(["-dimsu", "-t", &duration.as_secs().to_string()])
        .args(["-w", &std::process::id().to_string()])
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

#[cfg(test)]
mod tests {
    use super::*;

    fn seconds(input: &str) -> u64 {
        parse_duration(input).unwrap().as_secs()
    }

    #[test]
    fn reads_long_and_short_forms() {
        assert_eq!(seconds("30s"), 30);
        assert_eq!(seconds("15m"), 15 * 60);
        assert_eq!(seconds("2h"), 2 * 3_600);
        assert_eq!(seconds("5-minutes"), 5 * 60);
        assert_eq!(seconds("1-hour"), 3_600);
        assert_eq!(seconds("5 hours"), 5 * 3_600);
        assert_eq!(seconds("10_secs"), 10);
        assert_eq!(seconds("  3 HRS "), 3 * 3_600);
    }

    #[test]
    fn refuses_what_it_cannot_read() {
        for input in ["", "minutes", "0m", "5", "5d", "-", "1.5h"] {
            assert!(
                parse_duration(input).is_err(),
                "{input:?} should be refused"
            );
        }
    }

    #[test]
    fn refuses_more_than_a_year() {
        assert_eq!(seconds("8760h"), 365 * 24 * 3_600);
        assert!(parse_duration("8761h").is_err());
        assert!(parse_duration("99999999999999999999h").is_err());
    }

    #[test]
    fn formats_in_the_largest_whole_unit() {
        assert_eq!(format_duration(Duration::from_secs(1)), "1 second");
        assert_eq!(format_duration(Duration::from_secs(90)), "90 seconds");
        assert_eq!(format_duration(Duration::from_secs(120)), "2 minutes");
        assert_eq!(format_duration(Duration::from_secs(3_600)), "1 hour");
    }
}
