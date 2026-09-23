use std/assert

let repo = (mktemp --directory)
let old = ($repo | path join ".ctx" "opavs" "memory-bank")
mkdir $old
"# Active Context\n" | save ($old | path join "active-context.md")

nu scripts/migrate-memory-bank.nu $repo

let target = ($repo | path join ".ctx" "memory-bank" "active-context.md")
assert ($target | path exists)
assert not ($old | path exists)

nu scripts/migrate-memory-bank.nu $repo
assert equal (open --raw $target) "# Active Context\n"

rm --recursive $repo
