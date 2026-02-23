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

# Create deploys
glitchtip-cli deploys --release 1.0.0 new -e production

# Send a test event (uses SENTRY_DSN)
glitchtip-cli send-event -m "deploy verification" -l info -E production
```

## Configuration

Configuration is loaded in this order (highest priority first):

1. CLI flags (`--url`, `--auth-token`, `--org`, `--project`)
2. Environment variables (`SENTRY_URL`, `SENTRY_AUTH_TOKEN`, `SENTRY_ORG`, `SENTRY_PROJECT`, `SENTRY_DSN`)
3. Local config file (`.sentryclirc` or `.glitchtip-cli.rc`, searched upward from cwd)
4. Global config (`~/.config/glitchtip-cli/config`, `~/.config/sentry/sentrycli.ini`, or `~/.sentryclirc`)

Config files use INI format, matching sentry-cli:

```ini
[auth]
token=sntrys_xxx
dsn=https://key@your-glitchtip-instance.com/1

[defaults]
url=https://your-glitchtip-instance.com
org=my-org
project=my-project
```

## sentry-cli compatibility

GlitchTip CLI aims to be a drop-in replacement for the most common sentry-cli workflows: CI/CD release tracking, sourcemap uploads, and deploy notifications. It reads the same config files, environment variables, and accepts the same flags.

### Supported commands

| Command | Status | Notes |
|---|---|---|
| `login` | Supported | Interactive token setup |
| `info` | Supported | Config and auth status display |
| `releases new` | Supported | |
| `releases finalize` | Supported | |
| `releases list` | Supported | |
| `releases delete` | Supported | |
| `releases set-commits` | Supported | `--auto` for git discovery |
| `sourcemaps upload` | Supported | Chunked upload with gzip |
| `sourcemaps inject` | Supported | Inject debug IDs into source files and sourcemaps |
| `deploys new` | Supported | |
| `deploys list` | Supported | |
| `send-event` | Supported | DSN auth, envelope protocol |
| `organizations list` | Supported | |
| `projects list` | Supported | |
| `releases propose-version` | Supported | |
| `issues list` | Supported | |
| `issues resolve` | Supported | Bulk resolve by ID, status, or all |
| `issues mute` | Supported | Bulk mute (ignore) issues |
| `issues unresolve` | Supported | Bulk unresolve issues |
| `send-envelope` | Supported | Send a pre-built envelope file |

### Not yet supported

These sentry-cli commands are not yet implemented. They are listed roughly in order of priority.

| Command | Notes |
|---|---|
| `logs list` | List and filter log entries; GlitchTip supports logs |
| `monitors list` / `monitors run` | Cron monitor management and job wrapping |
| `events list` | List events for a project |
| `repos list` | List repositories for an organization |
| `releases info` | Print info about a specific release |
| `sourcemaps resolve` | Resolve sourcemap for a given line/column position |

### Not planned

These sentry-cli commands target use cases that are out of scope, either because they are niche, deprecated, or involve platform-specific toolchains.

| Command | Reason |
|---|---|
| `debug-files` | Native debug symbol management (dSYM, Proguard, etc.) |
| `react-native` | Platform-specific build tool integration |
| `react-native xcode` | macOS-only; platform-specific |
| `dart-symbol-map` | Dart/Flutter symbol deobfuscation; niche |
| `bash-hook` | Shell error hook; rarely used in practice |
| `send-metric` | Deprecated in sentry-cli |
| `update` / `uninstall` | Package manager territory |
| `sourcemaps explain` | Deprecated in sentry-cli |
| `releases archive` / `restore` | Not supported by GlitchTip backend |
| `files` | Deprecated in sentry-cli; use `sourcemaps upload` |
| `upload-dif` / `upload-dsym` / `upload-proguard` | Hidden aliases for `debug-files upload` |
| `build upload` | Niche build artifact management |

### Beyond sentry-cli

GlitchTip has features that sentry-cli doesn't cover. We plan to add CLI support for these where it makes sense.

| Feature | Notes |
|---|---|
| `monitors` (uptime) | CRUD for GlitchTip's uptime monitors — manage monitoring alongside deploys |
| `logs list --live` (streaming) | WebSocket-based live log streaming; sentry-cli only supports polling |

## License

MIT
