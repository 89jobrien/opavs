use crate::{domain, domain::TaskGraph};
use crate::{plugin, plugin::Target};
use anyhow::Result;
use std::path::{Path, PathBuf};

/// Read-only access to an artifact that may not exist.
pub trait ArtifactReader {
    /// Return an artifact's text, or `None` when the path does not exist.
    fn read(&self, path: &Path) -> Result<Option<String>>;

    /// Return Git's ignore decision, or `None` when the path is not in a Git repository.
    fn is_ignored(&self, _repo_root: &Path, _relative_path: &Path) -> Result<Option<bool>> {
        Ok(None)
    }
}

/// Severity assigned to a doctor finding.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FindingLevel {
    /// The inspected requirement is satisfied.
    Pass,
    /// Enforcement can continue, but configuration may be incomplete.
    Warning,
    /// Enforcement cannot be trusted until the finding is repaired.
    Error,
}

/// A read-only repair recommendation emitted by doctor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RepairAction {
    /// Initialize OPAVS state in the repository.
    RunInit { repo_root: PathBuf },
    /// Reinstall one supported client integration.
    InstallPlugin { target: Target, home: PathBuf },
    /// Apply a repair that has no safe automated command yet.
    Manual { description: String },
}

/// One diagnosed repository or client-integration requirement.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DoctorFinding {
    /// Stable identifier suitable for filtering diagnostic output.
    pub code: String,
    /// Severity of the finding.
    pub level: FindingLevel,
    /// Human-readable explanation.
    pub message: String,
    /// Optional advisory repair action.
    pub repair: Option<RepairAction>,
}

/// Complete read-only diagnosis for one repository and home directory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DoctorReport {
    /// Findings produced by all enabled doctor checks.
    pub findings: Vec<DoctorFinding>,
}

impl DoctorReport {
    /// Return whether any finding prevents trustworthy enforcement.
    pub fn has_errors(&self) -> bool {
        self.findings
            .iter()
            .any(|finding| finding.level == FindingLevel::Error)
    }
}

/// Inspect OPAVS repository state without changing files.
pub fn inspect(
    reader: &impl ArtifactReader,
    repo_root: &Path,
    home: &Path,
) -> Result<DoctorReport> {
    let active_context_path = repo_root.join(".ctx/opavs/memory-bank/active-context.md");
    let progress_path = repo_root.join(".ctx/opavs/memory-bank/progress.md");
    let opavs_path = repo_root.join("OPAVS.md");
    let agents = reader.read(&repo_root.join("AGENTS.md"))?;
    let claude = reader.read(&repo_root.join("CLAUDE.md"))?;
    let active_context = reader.read(&active_context_path)?;
    let progress = reader.read(&progress_path)?;
    let opavs = reader.read(&opavs_path)?;
    let has_partial_scaffold = active_context.is_some() || progress.is_some() || opavs.is_some();

    let tasks_path = repo_root.join(".ctx/opavs/tasks.yaml");
    let mut findings = vec![match reader.read(&tasks_path)? {
        None => DoctorFinding {
            code: "repo.tasks.missing".to_string(),
            level: FindingLevel::Error,
            message: format!("OPAVS task graph is missing at {}", tasks_path.display()),
            repair: Some(if has_partial_scaffold {
                RepairAction::Manual {
                    description: format!(
                        "repair missing task graph at {} and restore other missing scaffold files manually",
                        tasks_path.display()
                    ),
                }
            } else {
                RepairAction::RunInit {
                    repo_root: repo_root.to_path_buf(),
                }
            }),
        },
        Some(contents) => match serde_yaml::from_str::<TaskGraph>(&contents) {
            Ok(graph) => {
                if let Some(id) = duplicate_task_id(&graph) {
                    DoctorFinding {
                        code: "repo.tasks.duplicate_id".to_string(),
                        level: FindingLevel::Error,
                        message: format!("task graph contains duplicate id '{id}'"),
                        repair: Some(RepairAction::Manual {
                            description: format!(
                                "make task IDs unique in {}",
                                tasks_path.display()
                            ),
                        }),
                    }
                } else {
                    match domain::validate(&graph) {
                        Ok(()) => DoctorFinding {
                            code: "repo.tasks.valid".to_string(),
                            level: FindingLevel::Pass,
                            message: format!("task graph contains {} task(s)", graph.tasks.len()),
                            repair: None,
                        },
                        Err(error) => DoctorFinding {
                            code: "repo.tasks.invalid_graph".to_string(),
                            level: FindingLevel::Error,
                            message: format!("task graph is invalid: {error}"),
                            repair: Some(RepairAction::Manual {
                                description: format!(
                                    "repair task dependencies in {}",
                                    tasks_path.display()
                                ),
                            }),
                        },
                    }
                }
            }
            Err(error) => DoctorFinding {
                code: "repo.tasks.invalid".to_string(),
                level: FindingLevel::Error,
                message: format!("task graph cannot be parsed: {error}"),
                repair: Some(RepairAction::Manual {
                    description: format!("repair {} as valid task YAML", tasks_path.display()),
                }),
            },
        },
    }];

    for (path, contents, code, label) in [
        (
            active_context_path,
            active_context,
            "repo.memory.active_context.missing",
            "active context",
        ),
        (
            progress_path,
            progress,
            "repo.memory.progress.missing",
            "progress history",
        ),
        (
            opavs_path,
            opavs,
            "repo.opavs_md.missing",
            "OPAVS instructions",
        ),
    ] {
        findings.push(match contents {
            Some(_) => DoctorFinding {
                code: code.replace(".missing", ".present"),
                level: FindingLevel::Pass,
                message: format!("{label} exists at {}", path.display()),
                repair: None,
            },
            None => DoctorFinding {
                code: code.to_string(),
                level: FindingLevel::Error,
                message: format!("{label} is missing at {}", path.display()),
                repair: Some(RepairAction::Manual {
                    description: format!("restore {label} at {}", path.display()),
                }),
            },
        });
    }

    let instructions_linked = agents
        .as_deref()
        .is_some_and(|contents| contents.contains(crate::init::OPAVS_TEMPLATE))
        || claude.as_deref().is_some_and(|contents| {
            contents
                .lines()
                .any(|line| line.trim() == crate::init::OPAVS_LINK)
        });
    findings.push(if instructions_linked {
        DoctorFinding {
            code: "repo.instructions.linked".to_string(),
            level: FindingLevel::Pass,
            message: "agent instructions reference OPAVS".to_string(),
            repair: None,
        }
    } else {
        DoctorFinding {
            code: "repo.instructions.missing".to_string(),
            level: FindingLevel::Error,
            message: "AGENTS.md and CLAUDE.md do not reference OPAVS".to_string(),
            repair: Some(RepairAction::Manual {
                description:
                    "add the OPAVS instructions to AGENTS.md or link OPAVS.md from CLAUDE.md"
                        .to_string(),
            }),
        }
    });

    let phase_path = repo_root.join(".ctx/opavs/phase");
    if let Some(contents) = reader.read(&phase_path)? {
        findings.push(match domain::Phase::parse(contents.trim()) {
            Ok(phase) => DoctorFinding {
                code: "repo.phase.valid".to_string(),
                level: FindingLevel::Pass,
                message: format!("current phase is {phase}"),
                repair: None,
            },
            Err(error) => DoctorFinding {
                code: "repo.phase.invalid".to_string(),
                level: FindingLevel::Error,
                message: error.to_string(),
                repair: Some(RepairAction::Manual {
                    description: format!("remove or repair {}", phase_path.display()),
                }),
            },
        });
    }

    let gitignore_path = repo_root.join(".gitignore");
    let phase_ignored = match reader.is_ignored(repo_root, Path::new(".ctx/opavs/phase"))? {
        Some(ignored) => ignored,
        None => reader
            .read(&gitignore_path)?
            .as_deref()
            .is_some_and(gitignore_covers_phase),
    };
    findings.push(if phase_ignored {
        DoctorFinding {
            code: "repo.phase.ignored".to_string(),
            level: FindingLevel::Pass,
            message: "ephemeral phase state is gitignored".to_string(),
            repair: None,
        }
    } else {
        DoctorFinding {
            code: "repo.phase.not_ignored".to_string(),
            level: FindingLevel::Warning,
            message: ".ctx/opavs/phase is not gitignored".to_string(),
            repair: Some(RepairAction::Manual {
                description: format!("add .ctx/opavs/phase to {}", gitignore_path.display()),
            }),
        }
    });

    findings.extend(inspect_plugins(reader, home)?);

    Ok(DoctorReport { findings })
}

fn inspect_plugins(reader: &impl ArtifactReader, home: &Path) -> Result<Vec<DoctorFinding>> {
    let mut findings = Vec::new();
    for target in Target::ALL {
        let expectations = plugin::artifact_expectations(target, home);
        let mut installed = false;
        let mut drifted = Vec::new();

        for expectation in &expectations {
            match reader.read(&expectation.path)? {
                None => drifted.push(expectation.path.display().to_string()),
                Some(contents) => {
                    let current = expectation.is_current(&contents);
                    installed |= expectation.indicates_install(&contents);
                    if !current {
                        drifted.push(expectation.path.display().to_string());
                    }
                }
            }
        }

        let repair = || RepairAction::InstallPlugin {
            target,
            home: home.to_path_buf(),
        };
        findings.push(if !installed {
            DoctorFinding {
                code: format!("plugin.{}.missing", target.as_str()),
                level: FindingLevel::Warning,
                message: format!("{} integration is not installed", target.as_str()),
                repair: Some(repair()),
            }
        } else if drifted.is_empty() {
            DoctorFinding {
                code: format!("plugin.{}.current", target.as_str()),
                level: FindingLevel::Pass,
                message: format!("{} integration is current", target.as_str()),
                repair: None,
            }
        } else {
            DoctorFinding {
                code: format!("plugin.{}.drift", target.as_str()),
                level: FindingLevel::Error,
                message: format!(
                    "{} integration has {} missing or stale artifact(s): {}",
                    target.as_str(),
                    drifted.len(),
                    drifted.join(", ")
                ),
                repair: Some(repair()),
            }
        });
    }
    Ok(findings)
}

fn duplicate_task_id(graph: &TaskGraph) -> Option<&str> {
    let mut ids = std::collections::HashSet::new();
    graph
        .tasks
        .iter()
        .map(|task| task.id.as_str())
        .find(|id| !ids.insert(*id))
}

fn gitignore_covers_phase(contents: &str) -> bool {
    contents
        .lines()
        .any(|line| line.trim().trim_start_matches('/') == ".ctx/opavs/phase")
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TasksReader {
        tasks: &'static str,
    }

    impl ArtifactReader for TasksReader {
        fn read(&self, path: &Path) -> Result<Option<String>> {
            Ok(path
                .ends_with(".ctx/opavs/tasks.yaml")
                .then(|| self.tasks.to_string()))
        }
    }

    #[test]
    fn inspect_malformed_tasks_reports_error() {
        let report = inspect(
            &TasksReader {
                tasks: "tasks: [not-valid",
            },
            Path::new("/repo"),
            Path::new("/home"),
        )
        .expect("doctor report");

        assert!(report.has_errors());
        assert!(
            report
                .findings
                .iter()
                .any(|finding| finding.code == "repo.tasks.invalid")
        );
    }

    #[test]
    fn inspect_invalid_task_graph_reports_error() {
        let report = inspect(
            &TasksReader {
                tasks: "tasks:\n  - id: a\n    depends_on: [missing]\n",
            },
            Path::new("/repo"),
            Path::new("/home"),
        )
        .expect("doctor report");

        assert!(report.has_errors());
        assert!(
            report
                .findings
                .iter()
                .any(|finding| finding.code == "repo.tasks.invalid_graph")
        );
    }

    #[test]
    fn inspect_duplicate_task_ids_reports_error() {
        let report = inspect(
            &TasksReader {
                tasks: "tasks:\n  - id: duplicate\n  - id: duplicate\n",
            },
            Path::new("/repo"),
            Path::new("/home"),
        )
        .expect("doctor report");

        assert!(report.has_errors());
        assert!(
            report
                .findings
                .iter()
                .any(|finding| finding.code == "repo.tasks.duplicate_id")
        );
    }
}
