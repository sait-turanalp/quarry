// Gateway for profile-related tests

#[path = "profiles/test_manifest.rs"]
mod test_manifest;

#[path = "profiles/test_project_manifest.rs"]
mod test_project_manifest;

#[path = "profiles/test_local_overrides.rs"]
mod test_local_overrides;

#[path = "profiles/test_lockfile.rs"]
mod test_lockfile;

#[path = "profiles/test_resolver.rs"]
mod test_resolver;

#[path = "profiles/test_variables.rs"]
mod test_variables;

#[path = "profiles/test_installer.rs"]
mod test_installer;

#[path = "profiles/test_orchestrator.rs"]
mod test_orchestrator;

#[path = "profiles/test_template.rs"]
mod test_template;
