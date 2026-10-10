# slap

[![CI](https://github.com/realzai/slap/actions/workflows/ci.yml/badge.svg)](https://github.com/realzai/slap/actions/workflows/ci.yml)
[![crates.io](https://img.shields.io/crates/v/slap.svg)](https://crates.io/crates/slap)

Keep your Mac awake with a playful, space-themed terminal timer. `slap` is a friendly interactive layer over macOS [`caffeinate`](https://ss64.com/mac/caffeinate.html).

```text
 SLAP   MAC AWAKE SESSION

                         ≋=[◇]▶
                      .-""-.
                     / .::. \
                     \_____/

  AWAKE  04:38
  ━━━━━━━────────────────────
  caffeinate active  ·  lands at 10:30 PM
```

## Install

With the installer from the [latest release](https://github.com/realzai/slap/releases/latest),
which puts `slap` in `~/.cargo/bin`:

```sh
curl --proto '=https' --tlsv1.2 -LsSf https://github.com/realzai/slap/releases/latest/download/slap-installer.sh | sh
```

Or from [crates.io](https://crates.io/crates/slap), with [Rust](https://rustup.rs) 1.88 or newer:

```sh
cargo install slap
```

## Use

Long and short duration forms are supported:

```sh
slap 5-minutes
slap 1-hour
slap 5-hours
slap 30s
slap 15m
slap 2h
```

Press `q`, `Esc`, or `Ctrl-C` to end a session early. Press `i` while a session is running to check your internet connection — a Fast.com-powered speed test with a live gauge and a report in Mbps and MB pulled. If the ship can't reach mission control (no connection), the link check says so gracefully and `space` tries again. Press `i` or `Esc` to return to the timer; the awake session keeps running underneath.

When output is redirected or no interactive terminal is available, `slap` runs as a simple text command without the animation.

## Requirements

- macOS
- A terminal with color support

## Changes

[CHANGELOG.md](CHANGELOG.md) lists what changed in each release, and [RELEASING.md](RELEASING.md)
how a release is made.

## License

MIT
