#![no_main]

use libfuzzer_sys::fuzz_target;
use opavs::domain::TaskGraph;

// `import::read_external_graph` reads a user-supplied GODMODE.tasks.yaml
// path (external, not authored by this tool) and hands its contents to
// serde_yaml. This target skips the file-read wrapper and drives the parse
// directly against arbitrary bytes -- serde_yaml must never panic, only
// return Ok or Err.
fuzz_target!(|data: &[u8]| {
    let Ok(s) = std::str::from_utf8(data) else {
        return;
    };
    if let Ok(graph) = serde_yaml::from_str::<TaskGraph>(s) {
        // Round-trip invariant: whatever we successfully parsed must
        // re-serialize and re-parse to the same task count. A regression
        // here would mean serde_yaml (or our schema) silently drops data.
        let reserialized = serde_yaml::to_string(&graph).expect("serialize a parsed graph");
        let roundtripped: TaskGraph =
            serde_yaml::from_str(&reserialized).expect("reparse our own output");
        assert_eq!(graph.tasks.len(), roundtripped.tasks.len());
    }
});
