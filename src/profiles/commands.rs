//! CLI command definitions for profile management

use clap::Parser;
use std::path::PathBuf;

/// Profile management actions
#[derive(Debug, Clone, Parser)]
pub enum ProfileAction {
    /// Initialize project with a profile
    #[command(
        about = "Initialize project with a profile",
        after_help = "Examples:\n  quarry profile init claude\n  quarry profile init claude --source ~/.quarry/profiles"
    )]
    Init {
        /// Profile name to initialize
        profile_name: String,

        /// Profile source directory (defaults to ~/.quarry/profiles)
        #[arg(long)]
        source: Option<PathBuf>,

        /// Force initialization even if .quarry exists
        #[arg(short, long)]
        force: bool,
    },

    /// Install a profile to current workspace
    #[command(
        about = "Install a profile to current workspace",
        after_help = "Examples:\n  quarry profile install claude\n  quarry profile install claude --source git@github.com:quarry/profiles.git"
    )]
    Install {
        /// Profile name to install
        profile_name: String,

        /// Profile source (git URL or local directory)
        #[arg(long)]
        source: Option<String>,

        /// Git reference (branch, tag, or commit SHA)
        #[arg(long)]
        r#ref: Option<String>,

        /// Force installation even if profile exists
        #[arg(short, long)]
        force: bool,
    },

    /// List available profiles
    #[command(
        about = "List available profiles",
        after_help = "Example:\n  quarry profile list\n  quarry profile list --verbose"
    )]
    List {
        /// Show detailed information
        #[arg(short, long)]
        verbose: bool,

        /// Output in JSON format
        #[arg(long)]
        json: bool,
    },

    /// Show profile status for current workspace
    #[command(
        about = "Show active profile and installation status",
        after_help = "Example:\n  quarry profile status"
    )]
    Status {
        /// Show detailed file tracking information
        #[arg(short, long)]
        verbose: bool,
    },

    /// Sync team configuration
    #[command(
        about = "Register providers and install profiles from team configuration",
        after_help = "Examples:\n  quarry profile sync\n  quarry profile sync --force"
    )]
    Sync {
        /// Force installation even if profiles exist or files conflict
        #[arg(short, long)]
        force: bool,
    },

    /// Update an installed profile
    #[command(
        about = "Update an installed profile from its provider",
        after_help = "Examples:\n  quarry profile update quarry\n  quarry profile update quarry --force"
    )]
    Update {
        /// Profile name to update
        profile_name: String,

        /// Force update even if already at latest commit
        #[arg(short, long)]
        force: bool,
    },

    /// Remove an installed profile
    #[command(
        about = "Remove an installed profile from workspace",
        after_help = "Examples:\n  quarry profile remove quarry\n  quarry profile remove quarry --verbose"
    )]
    Remove {
        /// Profile name to remove
        profile_name: String,

        /// Show detailed removal information
        #[arg(short, long)]
        verbose: bool,
    },

    /// Manage profile providers
    #[command(
        about = "Manage profile providers",
        after_help = "Examples:\n  quarry profile provider add quarry/claude-provider\n  quarry profile provider add ./my-provider --id custom\n  quarry profile provider list\n  quarry profile provider list --verbose\n  quarry profile provider remove claude-provider"
    )]
    Provider {
        #[command(subcommand)]
        action: ProviderAction,
    },

    /// Verify profile integrity
    #[command(
        about = "Verify profile integrity",
        after_help = "Examples:\n  quarry profile verify claude\n  quarry profile verify --all\n  quarry profile verify --all --verbose"
    )]
    Verify {
        /// Profile name to verify (omit for --all)
        profile_name: Option<String>,

        /// Verify all installed profiles
        #[arg(long, conflicts_with = "profile_name")]
        all: bool,

        /// Show detailed verification information
        #[arg(short, long)]
        verbose: bool,
    },
}

/// Provider management actions
#[derive(Debug, Clone, Parser)]
pub enum ProviderAction {
    /// Add a provider to the global registry
    #[command(
        about = "Add a provider to the global registry",
        after_help = "Examples:\n  quarry profile provider add quarry/claude-provider\n  quarry profile provider add https://github.com/quarry/profiles.git\n  quarry profile provider add ./my-provider --id custom"
    )]
    Add {
        /// Provider source (GitHub shorthand, git URL, or local path)
        source: String,

        /// Custom provider ID (defaults to derived from source)
        #[arg(long)]
        id: Option<String>,
    },

    /// Remove a provider from the global registry
    #[command(
        about = "Remove a provider from the global registry",
        after_help = "Example:\n  quarry profile provider remove claude-provider"
    )]
    Remove {
        /// Provider ID to remove
        provider_id: String,
    },

    /// List registered providers
    #[command(
        about = "List registered providers",
        after_help = "Examples:\n  quarry profile provider list\n  quarry profile provider list --verbose"
    )]
    List {
        /// Show detailed information including available profiles
        #[arg(short, long)]
        verbose: bool,
    },
}
