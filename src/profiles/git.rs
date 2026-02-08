//! Git repository operations for profile fetching using libgit2
//!
//! Copied from plugins/resolver.rs to enable git-based profile sources

use super::error::{ProfileError, ProfileResult};
use git2::{
    AutotagOption, Cred, CredentialType, FetchOptions, ProxyOptions, RemoteCallbacks, Repository,
    build::RepoBuilder,
};
use std::path::Path;

/// Clone a repository with shallow depth using libgit2
pub fn clone_repository(
    repo_url: &str,
    target_dir: &Path,
    git_ref: Option<&str>,
) -> ProfileResult<String> {
    let is_local = repo_url.starts_with("file://") || Path::new(repo_url).exists();

    // Ensure parent directory exists
    if let Some(parent) = target_dir.parent() {
        std::fs::create_dir_all(parent)?;
    }

    // Remove target directory if it exists
    if target_dir.exists() {
        std::fs::remove_dir_all(target_dir)?;
    }

    // Set up callbacks for credentials
    let mut callbacks = RemoteCallbacks::new();
    callbacks.credentials(credential_callback);

    // Set up fetch options with shallow clone
    let mut fetch_opts = FetchOptions::new();
    if !is_local {
        fetch_opts.depth(1); // Shallow clone
    }
    fetch_opts.download_tags(AutotagOption::All);
    fetch_opts.remote_callbacks(callbacks);

    // Respect environment proxy settings
    let mut proxy_opts = ProxyOptions::new();
    proxy_opts.auto();
    fetch_opts.proxy_options(proxy_opts);

    // Build the clone operation
    let mut builder = RepoBuilder::new();
    builder.fetch_options(fetch_opts);

    // Specify branch/tag if provided
    if let Some(reference) = git_ref {
        builder.branch(reference);
    }

    // Perform the clone
    let repo =
        builder
            .clone(repo_url, target_dir)
            .map_err(|e| ProfileError::GitOperationFailed {
                operation: format!("clone {repo_url}: {e}"),
            })?;

    // Ensure proper checkout for the specified reference
    if let Some(reference) = git_ref {
        checkout_reference(&repo, reference)?;
    } else {
        // Ensure workdir is populated for default branch
        repo.checkout_head(None)
            .map_err(|e| ProfileError::GitOperationFailed {
                operation: format!("checkout head: {e}"),
            })?;
    }

    // Get and return the commit SHA
    get_commit_sha(target_dir)
}

/// Credential callback for git2 authentication
fn credential_callback(
    _url: &str,
    username_from_url: Option<&str>,
    allowed_types: CredentialType,
) -> Result<Cred, git2::Error> {
    // Try SSH key from agent first
    if allowed_types.is_ssh_key() {
        if let Ok(cred) = Cred::ssh_key_from_agent(username_from_url.unwrap_or("git")) {
            return Ok(cred);
        }
    }

    // Try default credentials (netrc, etc.)
    if let Ok(cred) = Cred::default() {
        return Ok(cred);
    }

    // Try username/password from environment
    if allowed_types.is_user_pass_plaintext() {
        if let (Ok(username), Ok(password)) =
            (std::env::var("GIT_USERNAME"), std::env::var("GIT_PASSWORD"))
        {
            return Cred::userpass_plaintext(&username, &password);
        }
    }

    Err(git2::Error::from_str("no credentials available"))
}

/// Checkout a specific reference (branch, tag, or commit)
fn checkout_reference(repo: &Repository, reference: &str) -> ProfileResult<()> {
    // Try to resolve the reference
    let obj = repo
        .revparse_single(reference)
        .map_err(|e| ProfileError::GitOperationFailed {
            operation: format!("could not resolve reference '{reference}': {e}"),
        })?;

    // Checkout the tree
    repo.checkout_tree(&obj, None)
        .map_err(|e| ProfileError::GitOperationFailed {
            operation: format!("checkout tree: {e}"),
        })?;

    // Set HEAD appropriately
    if obj.as_commit().is_some() {
        // For branches, set HEAD to the branch
        if repo.find_branch(reference, git2::BranchType::Local).is_ok() {
            repo.set_head(&format!("refs/heads/{reference}"))
                .map_err(|e| ProfileError::GitOperationFailed {
                    operation: format!("set head: {e}"),
                })?;
        } else {
            // For tags or specific SHAs, detached HEAD
            repo.set_head_detached(obj.id())
                .map_err(|e| ProfileError::GitOperationFailed {
                    operation: format!("set detached head: {e}"),
                })?;
        }
    }

    Ok(())
}

/// Get the current commit SHA of a repository
pub fn get_commit_sha(repo_dir: &Path) -> ProfileResult<String> {
    let repo = Repository::open(repo_dir).map_err(|e| ProfileError::GitOperationFailed {
        operation: format!("open repository: {e}"),
    })?;

    let head = repo.head().map_err(|e| ProfileError::GitOperationFailed {
        operation: format!("get HEAD: {e}"),
    })?;

    let commit = head
        .peel_to_commit()
        .map_err(|e| ProfileError::GitOperationFailed {
            operation: format!("peel to commit: {e}"),
        })?;

    Ok(commit.id().to_string())
}

/// Resolve a git reference to a commit SHA without cloning
pub fn resolve_reference(repo_url: &str, git_ref: &str) -> ProfileResult<String> {
    // Set up callbacks for credentials
    let mut callbacks = RemoteCallbacks::new();
    callbacks.credentials(credential_callback);

    // Create a temporary remote to ls-remote
    let mut remote =
        git2::Remote::create_detached(repo_url).map_err(|e| ProfileError::GitOperationFailed {
            operation: format!("create remote: {e}"),
        })?;

    // Connect to remote
    remote
        .connect_auth(git2::Direction::Fetch, Some(callbacks), None)
        .map_err(|e| ProfileError::GitOperationFailed {
            operation: format!("connect to remote: {e}"),
        })?;

    // List remote references
    let refs = remote
        .list()
        .map_err(|e| ProfileError::GitOperationFailed {
            operation: format!("list remote refs: {e}"),
        })?;

    // Find matching reference
    for remote_ref in refs {
        let name = remote_ref.name();
        // Match exact ref, branch, or tag
        if name == git_ref
            || name == format!("refs/heads/{git_ref}")
            || name == format!("refs/tags/{git_ref}")
        {
            return Ok(remote_ref.oid().to_string());
        }
    }

    Err(ProfileError::GitOperationFailed {
        operation: format!("reference '{git_ref}' not found in repository"),
    })
}

/// Check if a URL is a valid Git repository
pub fn validate_repository(repo_url: &str) -> ProfileResult<()> {
    // Set up callbacks for credentials
    let mut callbacks = RemoteCallbacks::new();
    callbacks.credentials(credential_callback);

    // Create a temporary remote
    let mut remote = git2::Remote::create_detached(repo_url)?;

    // Try to connect
    remote
        .connect_auth(git2::Direction::Fetch, Some(callbacks), None)
        .map_err(|e| {
            let err_msg = e.to_string();
            if err_msg.contains("not found") || err_msg.contains("does not exist") {
                ProfileError::ProviderNotFound {
                    provider: repo_url.to_string(),
                }
            } else {
                ProfileError::GitOperationFailed {
                    operation: format!("validate repository: {e}"),
                }
            }
        })?;

    // Disconnect
    remote.disconnect().ok();

    Ok(())
}
