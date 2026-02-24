mod api;
mod commands;
mod config;
mod oauth;
mod utils;

use std::path::PathBuf;
use std::process;
use std::time::{SystemTime, UNIX_EPOCH};

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(
    name = "glitchtip-cli",
    about = "CLI for GlitchTip",
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

        /// Login method: "token" (paste an API token) or "oauth" (browser-based OAuth)
        #[arg(long, default_value = "token")]
        method: String,
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

    /// Send a test event to GlitchTip
    #[command(name = "send-event")]
    SendEvent {
        /// Event message (can be specified multiple times, joined with newline)
        #[arg(long, short, action = clap::ArgAction::Append)]
        message: Vec<String>,

        /// Severity level: debug, info, warning, error, fatal
        #[arg(long, short, default_value = "error")]
        level: String,

        /// Release version
        #[arg(long, short)]
        release: Option<String>,

        /// Environment name
        #[arg(long, short = 'E')]
        env: Option<String>,

        /// Tag in KEY:VALUE format (can be specified multiple times)
        #[arg(long, short, action = clap::ArgAction::Append)]
        tag: Vec<String>,

        /// Extra data in KEY:VALUE format (can be specified multiple times)
        #[arg(long, short = 'e', action = clap::ArgAction::Append)]
        extra: Vec<String>,

        /// Platform name
        #[arg(long, default_value = "other")]
        platform: String,

        /// Fingerprint value (can be specified multiple times)
        #[arg(long, short, action = clap::ArgAction::Append)]
        fingerprint: Vec<String>,
    },

    /// Send a pre-built envelope file to GlitchTip
    #[command(name = "send-envelope")]
    SendEnvelope {
        /// Path to the envelope file
        #[arg(value_name = "PATH")]
        path: PathBuf,

        /// Send without validating envelope contents
        #[arg(long)]
        raw: bool,
    },

    /// Manage deploys
    Deploys {
        /// Release version
        #[arg(long, short, env = "SENTRY_RELEASE")]
        release: String,

        #[command(subcommand)]
        command: DeploysCommand,
    },

    /// Manage organizations
    Organizations {
        #[command(subcommand)]
        command: OrganizationsCommand,
    },

    /// Manage projects
    Projects {
        #[command(subcommand)]
        command: ProjectsCommand,
    },

    /// Manage issues
    Issues {
        #[command(subcommand)]
        command: IssuesCommand,
    },

    /// Manage events
    Events {
        #[command(subcommand)]
        command: EventsCommand,
    },
}

#[derive(Subcommand)]
enum OrganizationsCommand {
    /// List available organizations
    List,
}

#[derive(Subcommand)]
enum ProjectsCommand {
    /// List available projects
    List,
}

#[derive(Subcommand)]
enum IssuesCommand {
    /// List issues for a project
    List,

    /// Resolve issues
    Resolve {
        /// Issue ID to resolve (can be specified multiple times)
        #[arg(long, short, action = clap::ArgAction::Append)]
        id: Vec<u64>,

        /// Filter by current status
        #[arg(long, short)]
        status: Option<String>,

        /// Apply to all issues
        #[arg(long, short)]
        all: bool,
    },

    /// Mute (ignore) issues
    Mute {
        /// Issue ID to mute (can be specified multiple times)
        #[arg(long, short, action = clap::ArgAction::Append)]
        id: Vec<u64>,

        /// Filter by current status
        #[arg(long, short)]
        status: Option<String>,

        /// Apply to all issues
        #[arg(long, short)]
        all: bool,
    },

    /// Unresolve issues
    Unresolve {
        /// Issue ID to unresolve (can be specified multiple times)
        #[arg(long, short, action = clap::ArgAction::Append)]
        id: Vec<u64>,

        /// Filter by current status
        #[arg(long, short)]
        status: Option<String>,

        /// Apply to all issues
        #[arg(long, short)]
        all: bool,
    },
}

#[derive(Subcommand)]
enum EventsCommand {
    /// List events for a project
    List {
        /// Display the user column
        #[arg(long = "show-user", short = 'U')]
        show_user: bool,

        /// Display the tags column
        #[arg(long = "show-tags", short = 'T')]
        show_tags: bool,
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

    /// Print information about a release
    Info {
        /// Release version
        #[arg(allow_hyphen_values = true)]
        version: String,

        /// Display the projects column
        #[arg(long = "show-projects", short = 'P')]
        show_projects: bool,

        /// Display the commits column
        #[arg(long = "show-commits", short = 'C')]
        show_commits: bool,
    },

    /// Archive a release
    Archive {
        /// Release version
        #[arg(allow_hyphen_values = true)]
        version: String,
    },

    /// Restore an archived release
    Restore {
        /// Release version
        #[arg(allow_hyphen_values = true)]
        version: String,
    },

    /// Propose a version name based on git repository
    #[command(name = "propose-version")]
    ProposeVersion,
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

    /// Inject debug IDs into source files and sourcemaps
    Inject {
        /// Paths to search for source files
        #[arg(required = true)]
        paths: Vec<PathBuf>,

        /// File extensions to consider
        #[arg(long = "ext", short = 'x', action = clap::ArgAction::Append)]
        extensions: Vec<String>,

        /// Ignore patterns
        #[arg(long, short = 'i', action = clap::ArgAction::Append)]
        ignore: Vec<String>,

        /// Show what would be done without writing files
        #[arg(long = "dry-run")]
        dry_run: bool,
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

    // Refresh OAuth token at startup if needed (and no explicit --auth-token)
    if cli.auth_token.is_none() {
        if let Some(oauth_tokens) = &config.oauth {
            let now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs();
            // Refresh if token expires within 60 seconds
            if oauth_tokens.expires_at <= now + 60 {
                let http_client = reqwest::Client::new();
                match oauth::refresh_tokens(
                    &oauth_tokens.token_endpoint,
                    &oauth_tokens.refresh_token,
                    &oauth_tokens.client_id,
                    &http_client,
                )
                .await
                {
                    Ok(response) => {
                        let new_tokens = oauth::OAuthTokens {
                            access_token: response.access_token.clone(),
                            refresh_token: response
                                .refresh_token
                                .unwrap_or_else(|| oauth_tokens.refresh_token.clone()),
                            client_id: oauth_tokens.client_id.clone(),
                            expires_at: now + response.expires_in.unwrap_or(3600),
                            scopes: response
                                .scope
                                .unwrap_or_else(|| oauth_tokens.scopes.clone()),
                            token_endpoint: oauth_tokens.token_endpoint.clone(),
                        };
                        // Best-effort save — don't fail the command if save fails
                        let path = config::Config::config_path(true).unwrap_or_default();
                        let _ = config::Config::save_oauth_tokens(&path, &new_tokens);
                        config.auth_token = Some(response.access_token);
                        config.oauth = Some(new_tokens);
                    }
                    Err(e) => {
                        eprintln!("Warning: failed to refresh OAuth token: {e}");
                    }
                }
            }
        }
    }

    match cli.command {
        Commands::Login { global, method } => {
            commands::login::execute(&config, global, &method).await
        }
        Commands::Info {
            config_status_json,
            no_defaults,
        } => commands::info::execute(&config, config_status_json, no_defaults).await,
        Commands::SendEvent {
            message,
            level,
            release,
            env,
            tag,
            extra,
            platform,
            fingerprint,
        } => {
            let opts = commands::send_event::SendEventOptions {
                message: &message,
                level: &level,
                release: release.as_deref(),
                environment: env.as_deref(),
                tags: &tag,
                extra: &extra,
                platform: &platform,
                fingerprint: &fingerprint,
            };
            commands::send_event::execute(&config, &opts).await
        }
        Commands::SendEnvelope { path, raw } => {
            let opts = commands::send_envelope::SendEnvelopeOptions { path: &path, raw };
            commands::send_envelope::execute(&config, &opts).await
        }
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
            ReleasesCommand::Info {
                version,
                show_projects,
                show_commits,
            } => commands::releases::info(&config, &version, show_projects, show_commits).await,
            ReleasesCommand::Archive { version } => {
                commands::releases::archive(&config, &version).await
            }
            ReleasesCommand::Restore { version } => {
                commands::releases::restore(&config, &version).await
            }
            ReleasesCommand::ProposeVersion => {
                let version = commands::releases::propose_version()?;
                println!("{version}");
                Ok(())
            }
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
            SourcemapsCommand::Inject {
                paths,
                extensions,
                ignore,
                dry_run,
            } => {
                commands::sourcemaps::inject(&paths, &extensions, &ignore, dry_run)?;
                Ok(())
            }
        },
        Commands::Organizations { command } => match command {
            OrganizationsCommand::List => commands::organizations::list(&config).await,
        },
        Commands::Projects { command } => match command {
            ProjectsCommand::List => commands::projects::list(&config).await,
        },
        Commands::Events { command } => match command {
            EventsCommand::List {
                show_user,
                show_tags,
            } => commands::events::list(&config, show_user, show_tags).await,
        },
        Commands::Issues { command } => match command {
            IssuesCommand::List => commands::issues::list(&config).await,
            IssuesCommand::Resolve { id, status, all } => {
                commands::issues::resolve(&config, &id, status.as_deref(), all).await
            }
            IssuesCommand::Mute { id, status, all } => {
                commands::issues::mute(&config, &id, status.as_deref(), all).await
            }
            IssuesCommand::Unresolve { id, status, all } => {
                commands::issues::unresolve(&config, &id, status.as_deref(), all).await
            }
        },
    }
}
