# GlitchTip CLI

Open source CLI for [GlitchTip](https://glitchtip.com).

> **Warning:** This project is not yet stable. The API and behavior may change without notice until version 1.0.

## Installation

Download a binary from the [releases page](https://gitlab.com/glitchtip/glitchtip-cli/-/releases) or build from source:

```sh
cargo install --path .
```

This installs the `glitchtip-cli` binary.

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

Config files use INI format:

```ini
[auth]
token=sntrys_xxx
dsn=https://key@your-glitchtip-instance.com/1

[defaults]
url=https://your-glitchtip-instance.com
org=my-org
project=my-project
```

## Supported commands

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
| `repos list` | Supported | |
| `releases propose-version` | Supported | |
| `releases info` | Supported | Version, date, projects, commits |
| `releases archive` | Supported | |
| `releases restore` | Supported | |
| `issues list` | Supported | |
| `issues resolve` | Supported | Bulk resolve by ID, status, or all |
| `issues mute` | Supported | Bulk mute (ignore) issues |
| `issues unresolve` | Supported | Bulk unresolve issues |
| `send-envelope` | Supported | Send a pre-built envelope file |
| `events list` | Supported | With `--show-user` and `--show-tags` |
| `monitors list` | Supported | List uptime monitors |
| `monitors create` | Supported | Create an uptime monitor |
| `monitors delete` | Supported | Delete an uptime monitor |
| `monitors run` | Supported | Run a command and send heartbeat on success |
| `logs list` | Supported | List and filter OpenTelemetry log entries |
| `debug-files check` | Supported | Check debug info file for usability |
| `debug-files upload` | Supported | Upload debug information files (ELF/dSYM/PDB/etc.) |
| `debug-files find` | Supported | Locate debug files by debug ID |
| `debug-files print-sources` | Supported | Print source references in a debug file |
| `debug-files bundle-sources` | Supported | Create source bundles from debug info files |
| `debug-files bundle-jvm` | Supported | Create source bundles from Java/Kotlin sources |
| `dart-symbol-map upload` | Supported | Upload Dart/Flutter symbol maps for deobfuscation |

### Not yet supported

These commands are not yet implemented, listed roughly in order of priority.

| Command | Notes |
|---|---|
| `react-native gradle` | Upload sourcemaps from Android gradle builds |
| `sourcemaps resolve` | Resolve sourcemap for a given line/column position |

### Not planned

These commands are out of scope, either because they are niche, deprecated, or involve platform-specific toolchains.

| Command | Reason |
|---|---|
| `react-native xcode` | macOS-only |
| `react-native appcenter` | Deprecated |
| `bash-hook` | Shell error hook; rarely used in practice |
| `send-metric` | Deprecated |
| `update` / `uninstall` | Package manager territory |
| `sourcemaps explain` | Deprecated |
| `files` | Deprecated; use `sourcemaps upload` |
| `upload-dif` / `upload-dsym` / `upload-proguard` | Hidden aliases for `debug-files upload` |
| `build upload` | Niche build artifact management |

### GlitchTip-specific

Features unique to GlitchTip that we plan to add CLI support for.

| Feature | Notes |
|---|---|
| `logs list --live` (streaming) | WebSocket-based live log streaming |

## License

MIT
