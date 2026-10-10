# Changelog

What changed in each release of slap. Versions follow [Semantic Versioning](https://semver.org).

## [1.1.0](https://github.com/realzai/slap/releases/tag/v1.1.0) - 2026-10-11

The first release on crates.io and with prebuilt binaries.

### Added

- Press `i` during a session to check your internet connection: a speed test against Fast.com
  with a live gauge, then the speed, the data pulled, the latency and the server. Offline, it says
  so, and `space` tries again. The session keeps running underneath.

### Changed

- The timer uses less CPU: it works out when the session ends once, instead of on every frame.

### Fixed

- If slap is killed, caffeinate now stops with it, instead of keeping the Mac awake for the rest
  of the session.
- `slap --help` shows the real version.

## [1.0.0](https://github.com/realzai/slap/tree/v1.0.0) - 2026-09-26

The first version: keep the Mac awake for a duration such as `30m` or `2-hours`, with a
space-themed timer in the terminal, or as a plain command when output is not a terminal.
