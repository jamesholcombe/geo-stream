use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};

fn geo_stream_exe() -> PathBuf {
    if let Some(p) = std::env::var_os("CARGO_BIN_EXE_geo_stream") {
        let pb = PathBuf::from(p);
        if pb.exists() {
            return pb;
        }
    }
    let name = format!("geo-stream{}", std::env::consts::EXE_SUFFIX);
    let profile = if cfg!(debug_assertions) {
        "debug"
    } else {
        "release"
    };
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target").join(profile).join(name)
}

#[test]
fn geo_stream_sample_input_produces_enter_then_exit() {
    let bin = geo_stream_exe();
    assert!(
        bin.exists(),
        "geo-stream binary not found at {} (set CARGO_BIN_EXE_geo_stream or run `cargo test` from the workspace)",
        bin.display()
    );
    let manifest_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let sample_path = manifest_dir.join("../../examples/sample-input.ndjson");
    let input = fs::read_to_string(&sample_path).expect("read examples/sample-input.ndjson");

    let mut child = Command::new(&bin)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn geo-stream");

    {
        let mut stdin = child.stdin.take().expect("stdin");
        stdin.write_all(input.as_bytes()).expect("write stdin");
    }

    let output = child.wait_with_output().expect("wait on geo-stream");
    assert!(
        output.status.success(),
        "geo-stream exited with {}; stderr: {}",
        output.status,
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        output.stderr.is_empty(),
        "expected empty stderr, got: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8(output.stdout).expect("utf-8 stdout");
    let lines: Vec<&str> = stdout.lines().filter(|l| !l.is_empty()).collect();
    assert_eq!(lines.len(), 2, "stdout:\n{stdout}");

    let first: serde_json::Value = serde_json::from_str(lines[0]).expect("line 1 json");
    let second: serde_json::Value = serde_json::from_str(lines[1]).expect("line 2 json");

    assert_eq!(first["event"], "enter");
    assert_eq!(first["id"], "c1");
    assert_eq!(first["geofence"], "zone-1");

    assert_eq!(second["event"], "exit");
    assert_eq!(second["id"], "c1");
    assert_eq!(second["geofence"], "zone-1");
}
