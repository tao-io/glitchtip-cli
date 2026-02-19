mod api;
mod commands;
mod config;
mod utils;

use std::path::PathBuf;
use std::process;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(
    name = "glitchtip-cli",
    about = "CLI for GlitchTip (sentry-cli compatible)",
    version
)]
struct Cli {
    /// Sentry/GlitchTip server URL
    #[arg(long = "url", env = "SENTRY_URL")]
    url: Option<String>,

    /// Auth token
    #[arg(long = "auth-token", global = true, env = "SENTRY_AUTH_TOKEN")]
    auth_token: Option<String>,

    /// Organization slug
    #[arg(long, short, global = true, env = "SENTRY_ORG")]
    org: Option<String>,

    /// Project slug (can be specified multiple times)
    #[arg(long, short, global = true, action = clap::ArgAction::Append)]
    project: Vec<String>,

    /// Log level: trace, debug, info, warn, error
    #[arg(long = "log-level", global = true, env = "SENTRY_LOG_LEVEL")]
    log_level: Option<String>,

    /// Suppress all output
    #[arg(long, global = true)]
    quiet: bool,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Set up authentication token
    Login {
        /// Store token globally (~/.sentryclirc) instead of locally
        #[arg(long, short)]
        global: bool,
    },

    /// Show configuration and auth status
    Info {
        /// Output config status as JSON
        #[arg(long = "config-status-json")]
        config_status_json: bool,

        /// Skip org/project checks; only verify auth
        #[arg(long = "no-defaults")]
        no_defaults: bool,
    },

    /// Manage releases
    Releases {
        #[command(subcommand)]
        command: ReleasesCommand,
    },

    /// Manage source maps
    Sourcemaps {
        #[command(subcommand)]
        command: SourcemapsCommand,
    },

    /// Manage deploys
    Deploys {
        /// Release version
        #[arg(long, short, env = "SENTRY_RELEASE")]
        release: String,

        #[command(subcommand)]
        command: DeploysCommand,
    },
}

#[derive(Subcommand)]
enum ReleasesCommand {
    /// Create a new release
    New {
        /// Release version
        #[arg(allow_hyphen_values = true)]
        version: String,

        /// Optional URL for this release
        #[arg(long)]
        url: Option<String>,

        /// Immediately finalize the release
        #[arg(long)]
        finalize: bool,
    },

    /// Finalize a release
    Finalize {
        /// Release version
        #[arg(allow_hyphen_values = true)]
        version: String,

        /// Optional URL for this release
        #[arg(long)]
        url: Option<String>,

        /// Set release time (ISO 8601 or Unix timestamp); defaults to now
        #[arg(long)]
        released: Option<String>,
    },

    /// List releases
    List {
        /// Display the projects column
        #[arg(long = "show-projects", short = 'P')]
        show_projects: bool,

        /// Print raw delimiter-separated list
        #[arg(long, short = 'R')]
        raw: bool,

        /// Delimiter for --raw output
        #[arg(long, short = 'D', requires = "raw")]
        delimiter: Option<String>,
    },

    /// Delete a release
    Delete {
        /// Release version
        #[arg(allow_hyphen_values = true)]
        version: String,
    },

    /// Set commits for a release
    #[command(name = "set-commits")]
    SetCommits {
        /// Release version
        #[arg(allow_hyphen_values = true)]
        version: String,

        /// Automatically discover commits from git history
        #[arg(long)]
        auto: bool,

        /// Commit hash (can be specified multiple times)
        #[arg(long = "commit", short, action = clap::ArgAction::Append)]
        commits: Vec<String>,
    },
}

#[derive(Subcommand)]
enum DeploysCommand {
    /// Create a new deploy
    New {
        /// Environment name
        #[arg(long, short)]
        env: String,

        /// Optional URL for this deploy
        #[arg(long, short = 'u')]
        url: Option<String>,

        /// Deploy start time (ISO 8601)
        #[arg(long)]
        started: Option<String>,

        /// Deploy finish time (ISO 8601)
        #[arg(long)]
        finished: Option<String>,
    },

    /// List deploys
    List,
}

#[derive(Subcommand)]
enum SourcemapsCommand {
    /// Upload source maps
    Upload {
        /// Paths to search for source maps
        #[arg(required = true)]
        paths: Vec<PathBuf>,

        /// Release version
        #[arg(long, short, env = "SENTRY_RELEASE")]
        release: Option<String>,

        /// URL prefix to prepend to filenames
        #[arg(long = "url-prefix", short, default_value = "~")]
        url_prefix: String,

        /// Distribution identifier
        #[arg(long, short)]
        dist: Option<String>,

        /// File extensions to consider
        #[arg(long = "ext", short = 'x', action = clap::ArgAction::Append)]
        extensions: Vec<String>,

        /// Ignore patterns
        #[arg(long, short = 'i', action = clap::ArgAction::Append)]
        ignore: Vec<String>,

        /// Enable basic sourcemap validation
        #[arg(long)]
        validate: bool,

        /// Disable sourcemap rewriting
        #[arg(long = "no-rewrite")]
        no_rewrite: bool,
    },
}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();

    if let Err(e) = run(cli).await {
        if !e.to_string().is_empty() {
            eprintln!("error: {e:#}");
        }
        process::exit(1);
    }
}

async fn run(cli: Cli) -> anyhow::Result<()> {
    let mut config = config::Config::load()?;

    // Apply CLI overrides
    config.apply_cli_overrides(
        cli.auth_token.as_deref(),
        cli.url.as_deref(),
        cli.org.as_deref(),
        cli.project.first().map(|s| s.as_str()),
        cli.log_level.as_deref(),
    );

    match cli.command {
        Commands::Login { global } => commands::login::execute(&config, global).await,
        Commands::Info {
            config_status_json,
            no_defaults,
        } => commands::info::execute(&config, config_status_json, no_defaults).await,
        Commands::Releases { command } => match command {
            ReleasesCommand::New {
                version,
                url,
                finalize,
            } => {
                commands::releases::new_release(
                    &config,
                    &version,
                    &cli.project,
                    url.as_deref(),
                    finalize,
                )
                .await
            }
            ReleasesCommand::Finalize {
                version,
                url,
                released,
            } => {
                commands::releases::finalize(
                    &config,
                    &version,
                    &cli.project,
                    url.as_deref(),
                    released.as_deref(),
                )
                .await
            }
            ReleasesCommand::List {
                show_projects,
                raw,
                delimiter,
            } => {
                commands::releases::list(
                    &config,
                    cli.project.first().map(|s| s.as_str()),
                    show_projects,
                    raw,
                    delimiter.as_deref(),
                )
                .await
            }
            ReleasesCommand::Delete { version } => {
                commands::releases::delete(&config, &version).await
            }
            ReleasesCommand::SetCommits {
                version,
                auto,
                commits,
            } => commands::releases::set_commits(&config, &version, auto, &commits).await,
        },
        Commands::Deploys { release, command } => match command {
            DeploysCommand::New {
                env,
                url,
                started,
                finished,
            } => {
                commands::deploys::new_deploy(
                    &config,
                    &release,
                    &env,
                    url.as_deref(),
                    started.as_deref(),
                    finished.as_deref(),
                )
                .await
            }
            DeploysCommand::List => commands::deploys::list(&config, &release).await,
        },
        Commands::Sourcemaps { command } => match command {
            SourcemapsCommand::Upload {
                paths,
                release,
                url_prefix,
                dist,
                extensions,
                ignore,
                validate,
                no_rewrite,
            } => {
                let opts = commands::sourcemaps::UploadOptions {
                    paths: &paths,
                    release: release.as_deref(),
                    projects: &cli.project,
                    url_prefix: &url_prefix,
                    dist: dist.as_deref(),
                    extensions: &extensions,
                    ignore: &ignore,
                    validate,
                    no_rewrite,
                };
                commands::sourcemaps::upload(&config, &opts).await
            }
        },
    }
}
