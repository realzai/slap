# slap

[![CI](https://github.com/realzai/slap/actions/workflows/ci.yml/badge.svg)](https://github.com/realzai/slap/actions/workflows/ci.yml)

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

```sh
cargo install --path .
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

Press `q`, `Esc`, or `Ctrl-C` to end a session early. When output is redirected or no interactive terminal is available, `slap` runs as a simple text command without the animation.

## Requirements

- macOS
- A terminal with color support

## License

MIT
