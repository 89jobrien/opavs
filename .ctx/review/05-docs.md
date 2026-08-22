# Docs Accuracy Review

## Mismatches

- src/main.rs:29 -- doc comment on Command::Init reads `Scaffold .ctx/GODMODE.tasks.yaml, memory-bank/, and AGENTS.md` but the actual scaffolded path (src/init.rs:24, src/repo.rs:8, README.md:24, CLAUDE.md:25) is .ctx/opavs/tasks.yaml. GODMODE.tasks.yaml is only the name of the external import source file (src/import.rs:5, TasksAction::Import doc at main.rs:65-66), not what init creates. Stale/copy-pasted doc comment -- should say .ctx/opavs/tasks.yaml.

- src/init.rs:14-16 (AGENTS_TEMPLATE) -- the generated AGENTS.md text correctly says .ctx/opavs/tasks.yaml, confirming main.rs:29 is the outlier, not the code.

## Missing/weak doc comments

- src/adapters.rs -- FsPhaseStore, FsTaskStore, and their new/phase_file methods have no doc comments despite being public. PhaseStore/TaskStore trait methods (get, set, load, save in src/domain.rs:42-43,77-78) are undocumented too -- only the traits themselves have a one-line doc.
- src/domain.rs -- Phase::parse, Phase enum, Task, TaskGraph, GraphError are public with no doc comments.
- src/main.rs:169 run_guard() -- no doc comment explaining the hook JSON contract (tool_name/cwd/tool_input shape), despite being the most complex function in the crate.

## README coverage

- opavs tasks import (README.md:45-47) is accurately documented, upsert by id, status preserved, matches import.rs:11-13. No issue.
- README never documents opavs guard fail-open behavior when .ctx/opavs/tasks.yaml is missing (silently allows, main.rs:218-221) -- worth noting since it is a real security property.
- CLAUDE.md architecture file list (domain.rs, adapters.rs, guard.rs, repo.rs, init.rs, import.rs, main.rs) matches src/ exactly -- no mismatch there.

## Verdict

One concrete blocking mismatch (src/main.rs:29). Remaining items are doc-completeness suggestions, not inaccuracies.
