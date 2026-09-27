#!/usr/bin/env nu

# Move legacy memory-bank layouts into the shared .ctx/memory-bank directory.
export def main [repo: path = "."] {
    let root = ($repo | path expand)
    let target = ($root | path join ".ctx" "memory-bank")
    let legacy_dirs = [
        ($root | path join ".ctx" "opavs" "memory-bank")
        ($root | path join ".ctx" "godmode" "memory-bank")
        ($root | path join ".ctx" "memory-banking")
    ]

    mkdir $target

    for source in $legacy_dirs {
        if not ($source | path exists) {
            continue
        }

        for file in (glob ($source | path join "**" "*")) {
            if ($file | path type) != "file" {
                continue
            }

            let relative = ($file | path relative-to $source)
            let destination = ($target | path join $relative)
            if ($destination | path exists) {
                if (open --raw $file) != (open --raw $destination) {
                    error make {msg: $"refusing to overwrite conflicting file: ($destination)"}
                }
                rm $file
                continue
            }

            mkdir ($destination | path dirname)
            mv $file $destination
        }

        let remaining_files = (glob ($source | path join "**" "*") | where { |path| ($path | path type) == "file" } | length)
        if $remaining_files == 0 {
            rm --recursive $source
        }
    }

    print $"memory bank ready at ($target)"
}
