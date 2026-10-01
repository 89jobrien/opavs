# Active Context

## Verified

- Generated skill and SHIP workflow regressions are covered in `src/plugin.rs`.
- README, CLAUDE, CLI help, and smoke-skill descriptions now match current behavior.
- Formatting, compilation, clippy, all 102 tests, the end-to-end smoke flow, and diff
  whitespace checks pass.
- A specialist documentation re-review found no remaining or new drift.

## Next

- Review the full intended diff, then enter SHIP when ready to commit and push.
- Keep unrelated working-tree changes out of the shipping commit.

## 2026-10-01 session — no opavs source changed

This session worked in **34 sibling repos**, not in opavs. Nothing in `src/` was
touched. The following is opavs-relevant state only:

- `src/uninstall.rs` carries an uncommitted 174-line change (+163/-11) that was **not
  authored in this session**. It is on branch `develop`, not `main`. Do not fold it
  into an opavs commit without confirming its provenance — it directly affects the
  "keep unrelated working-tree changes out of the shipping commit" note above.
- `.local/` is untracked and was left alone.
- Branch is `develop`; HEAD is `61a59b6 flow: sync main into develop`.
- `godmode:whatidid` **cannot produce a report**: transcript recording is inactive.
  The newest `~/.claude/projects/**/*.jsonl` was last written 2026-08-23, so no
  session data exists for the current date. Session summaries have to come from
  `git log` until recording is restored. Worth fixing before relying on that skill.

## Cross-repo state this session left behind

- 63 Rust CLI binaries under `~/dev` verified to answer `--version` (was 18 of 127).
- 5 rescue branches are **committed but unmerged**, awaiting the user:
  `~/dev/{bazaar,braid,notfiles,obfsck,groovenance}-version-rescue`, each on
  `chore/version-flags`. Deleting the worktrees without merging discards the work.
- 4 warpx bins remain unverified behind a missing private `warp-channel-config`.
- 4 TUIs/daemons intentionally skipped.
- No regression gate exists for any `--version` flag. Recorded as the top
  speedup in `.ctx/godmode/reports/reflect/reflect-2026-10-01.md`.
