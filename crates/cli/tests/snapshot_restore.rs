mod common;

/// Verify that `--snapshot-file` + `--restore-from` preserves entity zone membership across
/// process restarts. Without restored state the second run cannot emit an Exit event because
/// it has no record of the entity being inside the zone.
#[test]
fn snapshot_restore_preserves_zone_membership() {
    let tmp = std::env::temp_dir().join("geo-stream-test-snapshot.json");
    // Clean up from any previous (failed) run.
    let _ = std::fs::remove_file(&tmp);

    // Run 1: register zone + move entity inside → expect Enter event + snapshot written.
    let run1_input = concat!(
        r#"{"type":"register_zone","id":"zone-1","polygon":{"type":"Polygon","coordinates":[[[0,0],[1,0],[1,1],[0,1],[0,0]]]}}"#,
        "\n",
        r#"{"type":"update","id":"c1","location":[0.5,0.5],"t":1000}"#,
        "\n"
    );
    let out1 = common::run_geo_stream_with_args(
        run1_input.as_bytes(),
        &["--snapshot-file", tmp.to_str().unwrap()],
    );
    common::assert_success_empty_stderr(&out1);

    let lines1 = common::stdout_event_lines(&out1);
    assert_eq!(
        lines1.len(),
        1,
        "run 1 stdout:\n{}",
        String::from_utf8_lossy(&out1.stdout)
    );
    let ev1: serde_json::Value = serde_json::from_str(&lines1[0]).expect("valid json");
    assert_eq!(ev1["event"], "enter", "expected Enter on run 1");
    assert_eq!(ev1["zone"], "zone-1");

    assert!(tmp.exists(), "snapshot file was not created");

    // Run 2: restore snapshot + move entity outside zone → expect Exit event.
    // No zone registration here — the zone must come from the restored snapshot.
    let run2_input = concat!(
        r#"{"type":"update","id":"c1","location":[5,5],"t":2000}"#,
        "\n"
    );
    let out2 = common::run_geo_stream_with_args(
        run2_input.as_bytes(),
        &["--restore-from", tmp.to_str().unwrap()],
    );
    common::assert_success_empty_stderr(&out2);

    let lines2 = common::stdout_event_lines(&out2);
    assert_eq!(
        lines2.len(),
        1,
        "expected Exit event from restored state; run 2 stdout:\n{}",
        String::from_utf8_lossy(&out2.stdout)
    );
    let ev2: serde_json::Value = serde_json::from_str(&lines2[0]).expect("valid json");
    assert_eq!(ev2["event"], "exit", "expected Exit on run 2");
    assert_eq!(ev2["zone"], "zone-1");
    assert_eq!(ev2["id"], "c1");

    let _ = std::fs::remove_file(&tmp);
}
