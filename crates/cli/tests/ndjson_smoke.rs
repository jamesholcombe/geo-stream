mod common;

use std::fs;

#[test]
fn geo_stream_sample_input_produces_enter_then_exit() {
    let bin = common::geo_stream_exe();
    assert!(
        bin.exists(),
        "geo-stream binary not found at {} (set CARGO_BIN_EXE_geo_stream or run `cargo test` from the workspace)",
        bin.display()
    );
    let manifest_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let sample_path = manifest_dir.join("../../examples/sample-input.ndjson");
    let input = fs::read_to_string(&sample_path).expect("read examples/sample-input.ndjson");

    let output = common::run_geo_stream(input.as_bytes());
    common::assert_success_empty_stderr(&output);

    let lines = common::stdout_event_lines(&output);
    assert_eq!(
        lines.len(),
        2,
        "stdout:\n{}",
        String::from_utf8_lossy(&output.stdout)
    );

    let first: serde_json::Value = serde_json::from_str(&lines[0]).expect("line 1 json");
    let second: serde_json::Value = serde_json::from_str(&lines[1]).expect("line 2 json");

    assert_eq!(first["event"], "enter");
    assert_eq!(first["id"], "c1");
    assert_eq!(first["zone"], "zone-1");
    assert_eq!(first["t"], 1000);

    assert_eq!(second["event"], "exit");
    assert_eq!(second["id"], "c1");
    assert_eq!(second["zone"], "zone-1");
    assert_eq!(second["t"], 2000);
}
