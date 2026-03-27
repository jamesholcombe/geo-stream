mod common;

use std::fs;
use std::path::Path;

fn fixture(name: &str) -> String {
    let p = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name);
    fs::read_to_string(&p).unwrap_or_else(|e| panic!("read {}: {e}", p.display()))
}

#[test]
fn catalog_overlap_picks_lexicographically_smallest_region() {
    let input = fixture("catalog_overlap.ndjson");
    let out = common::run_geo_stream(input.as_bytes());
    common::assert_success_empty_stderr(&out);
    let lines = common::stdout_event_lines(&out);
    assert_eq!(lines.len(), 1, "stdout: {:?}", lines);
    let v: serde_json::Value = serde_json::from_str(&lines[0]).unwrap();
    assert_eq!(v["event"], "assignment_changed");
    assert_eq!(v["id"], "e1");
    assert_eq!(v["region"], "a-region");
    assert_eq!(v["t"], 1700000000000u64);
}

#[test]
fn radius_boundary_inclusive_emits_approach_only() {
    let input = fixture("radius_boundary.ndjson");
    let out = common::run_geo_stream(input.as_bytes());
    common::assert_success_empty_stderr(&out);
    let lines = common::stdout_event_lines(&out);
    assert_eq!(lines.len(), 1, "stdout: {:?}", lines);
    let v: serde_json::Value = serde_json::from_str(&lines[0]).unwrap();
    assert_eq!(v["event"], "approach");
    assert_eq!(v["id"], "e1");
    assert_eq!(v["zone"], "r1");
}
