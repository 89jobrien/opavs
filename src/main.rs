mod adapters;
mod domain;
mod guard;
mod import;
mod init;
mod repo;

use adapters::{FsPhaseStore, FsTaskStore};
use anyhow::{Result, bail};
use clap::{Parser, Subcommand};
use domain::{Phase, PhaseStore, TaskStatus, TaskStore};
use std::env;
use std::io::Read;
use std::path::PathBuf;

#[derive(Parser)]
#[command(
    name = "opavs",
    version,
    about = "Orient-Plan-Act-Verify-Ship phase discipline CLI"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Scaffold .ctx/GODMODE.tasks.yaml, memory-bank/, and AGENTS.md in a repo.
    Init {
        #[arg(default_value = ".")]
        repo_root: PathBuf,
    },
    /// Get or set the current opavs phase.
    Phase {
        #[command(subcommand)]
        action: PhaseAction,
    },
    /// Inspect and mutate the task graph.
    Tasks {
        #[command(subcommand)]
        action: TasksAction,
    },
    /// PreToolUse hook entrypoint: reads Claude Code hook JSON on stdin,
    /// emits a permissionDecision JSON verdict on stdout.
    Guard,
}

#[derive(Subcommand)]
enum PhaseAction {
    Get,
    Set { phase: String },
}

#[derive(Subcommand)]
enum TasksAction {
    /// List all tasks with their status.
    List,
    /// List tasks that are runnable now (not done, all deps done).
    Runnable,
    /// Validate the task graph (unknown deps, cycles).
    Validate,
    /// Set a task's status.
    SetStatus { id: String, status: String },
    /// Merge an external GODMODE.tasks.yaml file into this repo's graph
    /// (upsert by id; existing task status is preserved).
    Import { path: PathBuf },
}

fn find_repo_root(explicit: Option<&PathBuf>) -> Result<PathBuf> {
    let start = match explicit {
        Some(p) => p.clone(),
        None => env::current_dir()?,
    };
    repo::resolve_repo_root(&start).ok_or_else(|| {
        anyhow::anyhow!(
            "no .ctx/opavs/tasks.yaml found walking up from {} -- run `opavs init` first",
            start.display()
        )
    })
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Command::Init { repo_root } => {
            let created = init::scaffold(&repo_root)?;
            for f in created {
                println!("created {f}");
            }
        }
        Command::Phase { action } => {
            let repo_root = find_repo_root(None)?;
            let store = FsPhaseStore::new(&repo_root);
            match action {
                PhaseAction::Get => println!("{}", store.get()?),
                PhaseAction::Set { phase } => {
                    let phase = Phase::parse(&phase)?;
                    store.set(phase)?;
                    println!("opavs phase -> {phase}");
                }
            }
        }
        Command::Tasks { action } => {
            let repo_root = find_repo_root(None)?;
            let store = FsTaskStore::new(&repo_root);
            match action {
                TasksAction::List => {
                    let graph = store.load()?;
                    for t in &graph.tasks {
                        println!("{:?}\t{}\t{}", t.status, t.id, t.description);
                    }
                }
                TasksAction::Runnable => {
                    let graph = store.load()?;
                    for t in domain::runnable_tasks(&graph) {
                        println!("{}\t{}", t.id, t.description);
                    }
                }
                TasksAction::Validate => {
                    let graph = store.load()?;
                    match domain::validate(&graph) {
                        Ok(()) => {
                            println!("ok: {} tasks, no unknown deps or cycles", graph.tasks.len())
                        }
                        Err(e) => bail!("{e}"),
                    }
                }
                TasksAction::SetStatus { id, status } => {
                    let mut graph = store.load()?;
                    let status = match status.as_str() {
                        "todo" => TaskStatus::Todo,
                        "in_progress" => TaskStatus::InProgress,
                        "done" => TaskStatus::Done,
                        other => bail!("invalid status: {other} (expected todo|in_progress|done)"),
                    };
                    let task = graph
                        .tasks
                        .iter_mut()
                        .find(|t| t.id == id)
                        .ok_or_else(|| anyhow::anyhow!("unknown task id: {id}"))?;
                    task.status = status.clone();
                    store.save(&graph)?;
                    println!("{id} -> {status:?}");
                }
                TasksAction::Import { path } => {
                    let incoming = import::read_external_graph(&path)?;
                    let base = store.load()?;
                    let merged = import::merge(&base, &incoming);
                    domain::validate(&merged).map_err(|e| anyhow::anyhow!("{e}"))?;
                    let added = merged.tasks.len() - base.tasks.len();
                    store.save(&merged)?;
                    println!(
                        "imported {} tasks from {} ({added} new, {} total)",
                        incoming.tasks.len(),
                        path.display(),
                        merged.tasks.len()
                    );
                }
            }
        }
        Command::Guard => run_guard()?,
    }

    Ok(())
}

fn run_guard() -> Result<()> {
    let mut input = String::new();
    std::io::stdin().read_to_string(&mut input)?;
    let hook: serde_json::Value = serde_json::from_str(&input)?;

    let tool = hook.get("tool_name").and_then(|v| v.as_str()).unwrap_or("");
    let session_cwd = hook
        .get("cwd")
        .and_then(|v| v.as_str())
        .map(String::from)
        .unwrap_or_else(|| env::var("PWD").unwrap_or_default());

    let (target_dir, is_commit_or_push): (Option<String>, bool) = match tool {
        "Edit" | "Write" => {
            let file_path = hook
                .pointer("/tool_input/file_path")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            if file_path.is_empty() {
                allow_and_exit();
            }
            let dir = PathBuf::from(file_path)
                .parent()
                .map(|p| p.display().to_string())
                .unwrap_or_default();
            (Some(dir), false)
        }
        "Bash" => {
            let cmd = hook
                .pointer("/tool_input/command")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            if !guard::command_touches_commit_or_push(cmd) {
                allow_and_exit();
            }
            let target = extract_dash_c_target(cmd).unwrap_or_else(|| session_cwd.clone());
            (Some(target), true)
        }
        _ => {
            allow_and_exit();
            (None, false)
        }
    };

    let Some(target_dir) = target_dir else {
        allow_and_exit();
        return Ok(());
    };

    let Some(repo_root) = repo::resolve_repo_root(&PathBuf::from(&target_dir)) else {
        allow_and_exit();
        return Ok(());
    };

    let store = FsPhaseStore::new(&repo_root);
    let phase = store.get().unwrap_or(Phase::Orient);

    match guard::decide(
        tool,
        is_commit_or_push,
        phase,
        &repo_root.display().to_string(),
    ) {
        guard::Verdict::Allow => println!("{{\"continue\": true}}"),
        guard::Verdict::Deny(reason) => {
            let out = serde_json::json!({
                "hookSpecificOutput": {
                    "hookEventName": "PreToolUse",
                    "permissionDecision": "deny",
                    "permissionDecisionReason": reason,
                }
            });
            println!("{out}");
        }
    }

    Ok(())
}

fn allow_and_exit() {
    println!("{{\"continue\": true}}");
    std::process::exit(0);
}

/// Extract the path argument to a `git -C <path>` flag, if present.
fn extract_dash_c_target(cmd: &str) -> Option<String> {
    let words: Vec<&str> = cmd.split_whitespace().collect();
    let pos = words.iter().position(|w| *w == "-C")?;
    words.get(pos + 1).map(|s| s.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_dash_c_target() {
        assert_eq!(
            extract_dash_c_target("git -C /repo push origin main"),
            Some("/repo".to_string())
        );
    }

    #[test]
    fn no_dash_c_target_returns_none() {
        assert_eq!(extract_dash_c_target("git commit -m x"), None);
    }
}
