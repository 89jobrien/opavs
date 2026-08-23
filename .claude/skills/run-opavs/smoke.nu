#!/usr/bin/env nu
# Drives the opavs CLI end to end against a throwaway scratch repo:
# init -> phase get/set -> tasks import/list/runnable/validate/set-status
# -> guard (Edit + git commit) across ORIENT/PLAN/ACT/SHIP.
#
# Usage: nu .claude/skills/run-opavs/smoke.nu [path/to/opavs/binary]
# Defaults to target/debug/opavs relative to the repo root (built if missing).

def main [bin_path?: string] {
    let repo_root = ($env.PWD)
    let bin = if $bin_path != null {
        $bin_path
    } else {
        $"($repo_root)/target/debug/opavs"
    }

    if not ($bin | path exists) {
        print "binary not found, building..."
        cargo build
    }

    let scratch = (mktemp -d)
    print $"=== scratch repo: ($scratch) ==="

    print "=== init ==="
    ^$bin init $scratch

    cd $scratch

    print "=== phase get (default) ==="
    ^$bin phase get

    print "=== tasks list (empty) ==="
    ^$bin tasks list

    let ext_tasks = $"($scratch)/GODMODE.tasks.yaml"
    "tasks:\n  - id: setup\n    description: Set up project scaffolding\n    status: todo\n    depends_on: []\n  - id: implement\n    description: Implement the feature\n    status: todo\n    depends_on: [setup]\n" | save $ext_tasks

    print "=== tasks import ==="
    ^$bin tasks import $ext_tasks

    print "=== tasks list ==="
    ^$bin tasks list

    print "=== tasks runnable (before) ==="
    ^$bin tasks runnable

    print "=== tasks validate ==="
    ^$bin tasks validate

    print "=== tasks set-status setup done ==="
    ^$bin tasks set-status setup done

    print "=== tasks runnable (after) ==="
    ^$bin tasks runnable

    print "=== phase set PLAN ==="
    ^$bin phase set PLAN

    print "=== guard: Edit in PLAN (expect deny) ==="
    ($"{\"tool_name\": \"Edit\", \"tool_input\": {\"file_path\": \"($scratch)/src/main.rs\"}, \"cwd\": \"($scratch)\"}" | ^$bin guard)

    print "=== phase set ACT ==="
    ^$bin phase set ACT

    print "=== guard: Edit in ACT (expect allow) ==="
    ($"{\"tool_name\": \"Edit\", \"tool_input\": {\"file_path\": \"($scratch)/src/main.rs\"}, \"cwd\": \"($scratch)\"}" | ^$bin guard)

    print "=== guard: git commit in ACT (expect deny) ==="
    ($"{\"tool_name\": \"Bash\", \"tool_input\": {\"command\": \"git commit -m x\"}, \"cwd\": \"($scratch)\"}" | ^$bin guard)

    print "=== phase set SHIP ==="
    ^$bin phase set SHIP

    print "=== guard: git commit in SHIP (expect allow) ==="
    ($"{\"tool_name\": \"Bash\", \"tool_input\": {\"command\": \"git commit -m x\"}, \"cwd\": \"($scratch)\"}" | ^$bin guard)

    cd $repo_root
    rm -rf $scratch
    print "=== smoke test complete, scratch repo cleaned up ==="
}
