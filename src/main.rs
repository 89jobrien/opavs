use anyhow::{Result, bail};
use clap::{Parser, Subcommand};
use opavs::adapters::{FsPhaseStore, FsTaskStore};
use opavs::domain::{self, Phase, PhaseStore, TaskStatus, TaskStore};
use opavs::{guard, import, init, repo};
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
    /// Scaffold .ctx/opavs/tasks.yaml, .ctx/opavs/memory-bank/, and AGENTS.md
    /// in a repo.
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
    /// Merge an external GODMODE.tasks.yaml file (same schema, not the same
    /// path/state convention) into this repo's graph (upsert by id; existing
    /// task status is preserved).
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
                    let status = TaskStatus::parse(&status).map_err(|e| anyhow::anyhow!("{e}"))?;
                    domain::set_status(&mut graph, &id, status.clone())
                        .map_err(|e| anyhow::anyhow!("{e}"))?;
                    store.save(&graph)?;
                    println!("{id} -> {status:?}");
                }
                TasksAction::Import { path } => {
                    let incoming = import::read_external_graph(&path)?;
                    let base = store.load()?;
                    let merged = import::merge(&base, &incoming);
                    domain::validate(&merged).map_err(|e| anyhow::anyhow!("{e}"))?;
                    let added = import::added_count(&base, &incoming);
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

/// What a PreToolUse hook call resolves to before any repo/phase lookup:
/// either an immediate allow (tool/command opavs doesn't gate), or something
/// that needs a repo root + phase to decide.
#[derive(Debug, PartialEq, Eq)]
enum GuardRequest {
    Allow,
    Check {
        tool: String,
        target_dir: String,
        is_commit_or_push: bool,
    },
}

/// Pure: parse a PreToolUse hook JSON payload into a `GuardRequest`. No I/O.
fn parse_guard_request(hook: &serde_json::Value, session_cwd: &str) -> GuardRequest {
    let tool = hook.get("tool_name").and_then(|v| v.as_str()).unwrap_or("");

    match tool {
        "Edit" | "Write" => {
            let file_path = hook
                .pointer("/tool_input/file_path")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            if file_path.is_empty() {
                return GuardRequest::Allow;
            }
            let target_dir = PathBuf::from(file_path)
                .parent()
                .map(|p| p.display().to_string())
                .unwrap_or_default();
            GuardRequest::Check {
                tool: tool.to_string(),
                target_dir,
                is_commit_or_push: false,
            }
        }
        "Bash" => {
            let cmd = hook
                .pointer("/tool_input/command")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            if !guard::command_touches_commit_or_push(cmd) {
                return GuardRequest::Allow;
            }
            let target_dir = extract_dash_c_target(cmd).unwrap_or_else(|| session_cwd.to_string());
            GuardRequest::Check {
                tool: tool.to_string(),
                target_dir,
                is_commit_or_push: true,
            }
        }
        _ => GuardRequest::Allow,
    }
}

const ALLOW_JSON: &str = "{\"continue\": true}";

/// Pure (given `resolve`): decide the JSON verdict to print for a hook call.
/// `resolve` maps a target directory to `(repo_root, current_phase)` — the
/// one impure lookup, injected so this function is unit-testable without
/// touching the filesystem.
fn evaluate_guard(
    hook: &serde_json::Value,
    session_cwd: &str,
    resolve: impl Fn(&str) -> Option<(PathBuf, Phase)>,
) -> String {
    let request = parse_guard_request(hook, session_cwd);

    let GuardRequest::Check {
        tool,
        target_dir,
        is_commit_or_push,
    } = request
    else {
        return ALLOW_JSON.to_string();
    };

    let Some((repo_root, phase)) = resolve(&target_dir) else {
        return ALLOW_JSON.to_string();
    };

    match guard::decide(
        &tool,
        is_commit_or_push,
        phase,
        &repo_root.display().to_string(),
    ) {
        guard::Verdict::Allow => ALLOW_JSON.to_string(),
        guard::Verdict::Deny(reason) => serde_json::json!({
            "hookSpecificOutput": {
                "hookEventName": "PreToolUse",
                "permissionDecision": "deny",
                "permissionDecisionReason": reason,
            }
        })
        .to_string(),
    }
}

fn run_guard() -> Result<()> {
    let mut input = String::new();
    std::io::stdin().read_to_string(&mut input)?;
    let hook: serde_json::Value = serde_json::from_str(&input)?;
    let session_cwd = hook
        .get("cwd")
        .and_then(|v| v.as_str())
        .map(String::from)
        .unwrap_or_else(|| env::var("PWD").unwrap_or_default());

    let out = evaluate_guard(&hook, &session_cwd, |target_dir| {
        let repo_root = repo::resolve_repo_root(&PathBuf::from(target_dir))?;
        let phase = FsPhaseStore::new(&repo_root).get().unwrap_or(Phase::Orient);
        Some((repo_root, phase))
    });

    println!("{out}");
    Ok(())
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

    fn edit_hook(file_path: &str) -> serde_json::Value {
        serde_json::json!({"tool_name": "Edit", "tool_input": {"file_path": file_path}})
    }

    fn bash_hook(command: &str) -> serde_json::Value {
        serde_json::json!({"tool_name": "Bash", "tool_input": {"command": command}})
    }

    #[test]
    fn parse_guard_request_edit_with_empty_path_allows() {
        assert_eq!(
            parse_guard_request(&edit_hook(""), "/cwd"),
            GuardRequest::Allow
        );
    }

    #[test]
    fn parse_guard_request_edit_yields_check() {
        let req = parse_guard_request(&edit_hook("/repo/src/main.rs"), "/cwd");
        assert_eq!(
            req,
            GuardRequest::Check {
                tool: "Edit".to_string(),
                target_dir: "/repo/src".to_string(),
                is_commit_or_push: false,
            }
        );
    }

    #[test]
    fn parse_guard_request_bash_non_commit_allows() {
        assert_eq!(
            parse_guard_request(&bash_hook("cargo test"), "/cwd"),
            GuardRequest::Allow
        );
    }

    #[test]
    fn parse_guard_request_bash_commit_yields_check_with_dash_c_target() {
        let req = parse_guard_request(&bash_hook("git -C /repo commit -m x"), "/cwd");
        assert_eq!(
            req,
            GuardRequest::Check {
                tool: "Bash".to_string(),
                target_dir: "/repo".to_string(),
                is_commit_or_push: true,
            }
        );
    }

    #[test]
    fn parse_guard_request_bash_commit_falls_back_to_session_cwd() {
        let req = parse_guard_request(&bash_hook("git commit -m x"), "/session/cwd");
        assert_eq!(
            req,
            GuardRequest::Check {
                tool: "Bash".to_string(),
                target_dir: "/session/cwd".to_string(),
                is_commit_or_push: true,
            }
        );
    }

    #[test]
    fn parse_guard_request_unrelated_tool_allows() {
        let hook = serde_json::json!({"tool_name": "Read"});
        assert_eq!(parse_guard_request(&hook, "/cwd"), GuardRequest::Allow);
    }

    #[test]
    fn parse_guard_request_missing_tool_name_allows() {
        let hook = serde_json::json!({});
        assert_eq!(parse_guard_request(&hook, "/cwd"), GuardRequest::Allow);
    }

    #[test]
    fn evaluate_guard_allows_when_request_is_allow() {
        let out = evaluate_guard(&bash_hook("cargo test"), "/cwd", |_| unreachable!());
        assert_eq!(out, ALLOW_JSON);
    }

    #[test]
    fn evaluate_guard_allows_when_resolve_finds_no_repo() {
        let out = evaluate_guard(&edit_hook("/nowhere/file.rs"), "/cwd", |_| None);
        assert_eq!(out, ALLOW_JSON);
    }

    #[test]
    fn evaluate_guard_allows_edit_in_act_phase() {
        let out = evaluate_guard(&edit_hook("/repo/src/main.rs"), "/cwd", |_| {
            Some((PathBuf::from("/repo"), Phase::Act))
        });
        assert_eq!(out, ALLOW_JSON);
    }

    #[test]
    fn evaluate_guard_denies_edit_outside_act_phase() {
        let out = evaluate_guard(&edit_hook("/repo/src/main.rs"), "/cwd", |_| {
            Some((PathBuf::from("/repo"), Phase::Orient))
        });
        assert!(out.contains("\"permissionDecision\":\"deny\""));
        assert!(out.contains("ACT"));
    }

    #[test]
    fn evaluate_guard_denies_commit_outside_ship_phase() {
        let out = evaluate_guard(&bash_hook("git commit -m x"), "/cwd", |_| {
            Some((PathBuf::from("/repo"), Phase::Act))
        });
        assert!(out.contains("\"permissionDecision\":\"deny\""));
        assert!(out.contains("SHIP"));
    }

    #[test]
    fn evaluate_guard_allows_commit_in_ship_phase() {
        let out = evaluate_guard(&bash_hook("git commit -m x"), "/cwd", |_| {
            Some((PathBuf::from("/repo"), Phase::Ship))
        });
        assert_eq!(out, ALLOW_JSON);
    }
}

#[cfg(test)]
mod proptests {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        /// No arbitrary UTF-8 input may panic the extractor.
        #[test]
        fn extract_dash_c_target_never_panics(cmd in ".*") {
            let _ = extract_dash_c_target(&cmd);
        }

        /// When "-C <target>" appears anywhere, it must be extracted verbatim,
        /// regardless of what surrounds it.
        #[test]
        fn extracts_the_word_following_dash_c(
            prefix in "[a-zA-Z ]{0,10}",
            target in "[a-zA-Z0-9/_-]{1,10}",
            suffix in "[a-zA-Z ]{0,10}"
        ) {
            let cmd = format!("{prefix} -C {target} {suffix}");
            prop_assert_eq!(extract_dash_c_target(&cmd), Some(target));
        }

        /// Arbitrary JSON values must never panic hook parsing, regardless of
        /// tool_name/tool_input shape.
        #[test]
        fn parse_guard_request_never_panics_on_arbitrary_shapes(
            tool_name in prop::option::of("[a-zA-Z]{0,8}"),
            command in prop::option::of(".*"),
        ) {
            let mut hook = serde_json::json!({});
            if let Some(t) = tool_name {
                hook["tool_name"] = serde_json::json!(t);
            }
            if let Some(c) = command {
                hook["tool_input"] = serde_json::json!({"command": c});
            }
            let _ = parse_guard_request(&hook, "/cwd");
        }
    }
}
