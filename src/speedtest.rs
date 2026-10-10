use std::{
    collections::VecDeque,
    io::Read,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc::Sender,
    },
    thread,
    time::{Duration, Instant},
};

use anyhow::{Context, Result, bail};
use reqwest::{blocking::Client, header};
use serde::Deserialize;

const API_ENDPOINT: &str = "https://api.fast.com/netflix/speedtest/v2";
const FALLBACK_TOKEN: &str = "YXNkZmFzZGxmbnNkYWZoYXNkZmhrYWxm";
pub const TEST_DURATION: Duration = Duration::from_secs(10);
const WORKERS: usize = 6;
const CHUNK_SIZE: usize = 64 * 1024;

#[derive(Clone, Debug)]
pub struct TestResult {
    pub speed_mbps: f64,
    pub latency_ms: u64,
    pub bytes: u64,
    pub server: String,
}

#[derive(Clone, Debug)]
pub enum Update {
    Phase(&'static str),
    Sample { speed_mbps: f64, elapsed: Duration },
    Finished(TestResult),
    Failed(String),
}

#[derive(Debug, Deserialize)]
struct ApiResponse {
    targets: Vec<Target>,
}

#[derive(Clone, Debug, Deserialize)]
struct Target {
    url: String,
    location: Location,
}

#[derive(Clone, Debug, Deserialize)]
struct Location {
    city: String,
    country: String,
}

impl Location {
    fn label(&self) -> String {
        format!("{}, {}", self.city, self.country)
    }
}

/// Runs a full speed test on the calling thread and reports progress over
/// `sender`. Never panics; failures are reported as `Update::Failed`.
pub fn run(sender: Sender<Update>) {
    if let Err(error) = run_inner(&sender) {
        send(&sender, Update::Failed(error.to_string()));
    }
}

fn run_inner(sender: &Sender<Update>) -> Result<()> {
    let client = Client::builder()
        .user_agent(concat!("slap/", env!("CARGO_PKG_VERSION")))
        .connect_timeout(Duration::from_secs(8))
        .timeout(Duration::from_secs(20))
        .build()
        .context("could not create the HTTP client")?;

    send(sender, Update::Phase("Finding the nearest Netflix edge"));
    let token = discover_token(&client).unwrap_or_else(|_| FALLBACK_TOKEN.to_owned());
    let api = fetch_targets(&client, &token)?;
    let target = api
        .targets
        .first()
        .context("Fast.com returned no test servers")?;

    send(sender, Update::Phase("Measuring unloaded latency"));
    let latency_ms = measure_latency(&client, &target.url)?;

    send(sender, Update::Phase("Opening the taps"));
    let (speed_mbps, bytes) = measure_download(&client, &api.targets, sender)?;

    send(
        sender,
        Update::Finished(TestResult {
            speed_mbps,
            latency_ms,
            bytes,
            server: target.location.label(),
        }),
    );

    Ok(())
}

fn discover_token(client: &Client) -> Result<String> {
    let html = client
        .get("https://fast.com/")
        .send()?
        .error_for_status()?
        .text()?;
    let script_path = html
        .split("src=\"")
        .filter_map(|part| part.split_once('"').map(|(path, _)| path))
        .find(|path| path.starts_with("/app-") && path.ends_with(".js"))
        .context("could not find the Fast.com application script")?;
    let script = client
        .get(format!("https://fast.com{script_path}"))
        .send()?
        .error_for_status()?
        .text()?;

    extract_token(&script).context("could not find the Fast.com API token")
}

fn extract_token(script: &str) -> Option<String> {
    ["token:\"", "token: \"", "token='", "token = '"]
        .iter()
        .find_map(|marker| {
            let rest = script.split_once(marker)?.1;
            let quote = if marker.ends_with('\'') { '\'' } else { '"' };
            let token = rest.split(quote).next()?;
            (token.len() >= 20
                && token
                    .chars()
                    .all(|char| char.is_ascii_alphanumeric() || "-_".contains(char)))
            .then(|| token.to_owned())
        })
}

fn fetch_targets(client: &Client, token: &str) -> Result<ApiResponse> {
    let response = client
        .get(format!(
            "{API_ENDPOINT}?https=true&token={token}&urlCount=5"
        ))
        .send()
        .context("could not reach Fast.com")?
        .error_for_status()
        .context("Fast.com rejected the test request")?
        .json::<ApiResponse>()
        .context("Fast.com returned an unexpected response")?;

    if response.targets.is_empty() {
        bail!("Fast.com returned no test servers");
    }
    Ok(response)
}

fn measure_latency(client: &Client, url: &str) -> Result<u64> {
    let mut measurements = Vec::with_capacity(4);

    for attempt in 0..4 {
        let started = Instant::now();
        client
            .get(url)
            .header(header::RANGE, "bytes=0-0")
            .send()
            .context("the test server did not respond")?
            .error_for_status()
            .context("the test server rejected the latency probe")?;
        if attempt > 0 {
            measurements.push(started.elapsed().as_secs_f64() * 1_000.0);
        }
    }

    measurements.sort_by(f64::total_cmp);
    Ok(measurements[measurements.len() / 2].round() as u64)
}

fn measure_download(
    client: &Client,
    targets: &[Target],
    sender: &Sender<Update>,
) -> Result<(f64, u64)> {
    let bytes = Arc::new(AtomicU64::new(0));
    let stop = Arc::new(AtomicBool::new(false));

    for worker_id in 0..WORKERS {
        let client = client.clone();
        let target = targets[worker_id % targets.len()].url.clone();
        let bytes = Arc::clone(&bytes);
        let stop = Arc::clone(&stop);
        thread::spawn(move || download_worker(client, target, bytes, stop));
    }

    let started = Instant::now();
    let mut window = VecDeque::<(Instant, u64)>::new();
    let mut stable_samples = Vec::new();

    loop {
        thread::sleep(Duration::from_millis(200));
        let now = Instant::now();
        let total = bytes.load(Ordering::Relaxed);
        window.push_back((now, total));
        while window.len() > 2
            && now.duration_since(window.front().expect("window is not empty").0)
                > Duration::from_secs(1)
        {
            window.pop_front();
        }

        let speed_mbps = window.front().map_or(0.0, |(past, past_bytes)| {
            let seconds = now.duration_since(*past).as_secs_f64();
            if seconds == 0.0 {
                0.0
            } else {
                (total - past_bytes) as f64 * 8.0 / seconds / 1_000_000.0
            }
        });
        let elapsed = started.elapsed();
        if elapsed >= Duration::from_secs(2) && speed_mbps > 0.0 {
            stable_samples.push(speed_mbps);
        }
        send(
            sender,
            Update::Sample {
                speed_mbps,
                elapsed,
            },
        );

        if elapsed >= TEST_DURATION {
            break;
        }
    }

    stop.store(true, Ordering::Relaxed);

    if stable_samples.is_empty() {
        bail!("the test completed without receiving data");
    }
    stable_samples.sort_by(f64::total_cmp);
    let trim = stable_samples.len() / 10;
    let samples = &stable_samples[trim..stable_samples.len().saturating_sub(trim).max(trim + 1)];
    let average = samples.iter().sum::<f64>() / samples.len() as f64;

    Ok((average, bytes.load(Ordering::Relaxed)))
}

fn download_worker(client: Client, url: String, bytes: Arc<AtomicU64>, stop: Arc<AtomicBool>) {
    let mut buffer = vec![0_u8; CHUNK_SIZE];
    while !stop.load(Ordering::Relaxed) {
        let response = client
            .get(&url)
            .send()
            .and_then(|response| response.error_for_status());
        let Ok(mut response) = response else {
            return;
        };
        loop {
            match response.read(&mut buffer) {
                Ok(0) => break,
                Ok(count) => {
                    bytes.fetch_add(count as u64, Ordering::Relaxed);
                    if stop.load(Ordering::Relaxed) {
                        return;
                    }
                }
                Err(_) => return,
            }
        }
    }
}

fn send(sender: &Sender<Update>, update: Update) {
    let _ = sender.send(update);
}
