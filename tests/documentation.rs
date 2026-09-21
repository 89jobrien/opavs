//! Documentation contract tests for user guides and architecture design.

use std::{fs, path::PathBuf};

fn repo_file(path: &str) -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(path);
    fs::read_to_string(&path).unwrap_or_else(|error| panic!("read {}: {error}", path.display()))
}

fn assert_concepts(document: &str, document_name: &str, concepts: &[&str]) {
    let normalized_document = document.to_lowercase();
    let missing: Vec<_> = concepts
        .iter()
        .filter(|concept| !normalized_document.contains(&concept.to_lowercase()))
        .copied()
        .collect();
    assert!(
        missing.is_empty(),
        "{document_name} is missing documentation contract concepts: {}",
        missing.join(", ")
    );
}

#[test]
fn design_documents_reader_ignore_failure_and_effect_contracts() {
    let design = repo_file("docs/designs/2026-09-07-doctor-repair-planner-design.md");

    assert_concepts(
        &design,
        "doctor repair planner design",
        &[
            "ArtifactReader",
            "fn read(&self",
            "Result<Option<String>>",
            "pub trait IgnoreQuery",
            "fn is_ignored(",
            "Result<Option<bool>>",
            "reader: &dyn ArtifactReader",
            "ignore: &dyn IgnoreQuery",
            "catalog: &dyn IntegrationCatalog",
            "Ok(None)",
            "Some(true)",
            "Some(false)",
            ".gitignore",
            "propagate",
            "setup",
            "inspection",
            "nonzero",
            "git check-ignore",
            "GitIgnoreQuery",
            "read-only",
        ],
    );

    assert!(
        !design.contains("They are currently methods on `ArtifactReader`"),
        "design must describe the implemented split ports"
    );
    assert!(
        !design.contains(
            "`FsArtifactReader` in `opavs::adapters` reads filesystem artifacts and may launch"
        ),
        "design must attribute Git subprocesses to GitIgnoreQuery"
    );
}

#[test]
fn claude_documents_doctor_adapter_and_integration_architecture() {
    let claude = repo_file("CLAUDE.md");

    assert_concepts(
        &claude,
        "CLAUDE.md",
        &[
            "src/doctor.rs",
            "FsArtifactReader",
            "filesystem",
            "Git",
            "owned",
            "shared",
        ],
    );
}

#[test]
fn readme_documents_doctor_behavior_and_architecture() {
    let readme = repo_file("README.md");

    assert_concepts(
        &readme,
        "README.md",
        &[
            "opavs doctor",
            "Pass",
            "Warning",
            "Error",
            "advisory",
            "does not apply",
            "setup",
            "inspection",
            "nonzero",
            "filesystem",
            "Git",
        ],
    );
}
