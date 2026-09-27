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
