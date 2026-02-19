# GlitchTip CLI

Open source CLI for [GlitchTip](https://glitchtip.com), compatible with sentry-cli.

> **Warning:** This project is not yet stable. The API and behavior may change without notice until version 1.0.

## Installation

Download a binary from the [releases page](https://gitlab.com/glitchtip/glitchtip-cli/-/releases) or build from source:

```sh
cargo install --path .
```

This installs two binaries: `glitchtip-cli` and `sentry-cli` (drop-in compatibility alias).

## Usage

```sh
# Configure
glitchtip-cli --url https://your-glitchtip-instance.com login

# Upload sourcemaps
glitchtip-cli sourcemaps upload --release 1.0.0 ./dist

# Manage releases
glitchtip-cli releases list
glitchtip-cli releases new 1.0.0
glitchtip-cli releases finalize 1.0.0
glitchtip-cli releases set-commits 1.0.0 --auto
glitchtip-cli releases deploys new 1.0.0 -e production
```

## License

MIT
