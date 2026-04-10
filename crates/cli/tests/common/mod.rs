//! Shared helpers for `geo-stream` integration tests.

use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Output, Stdio};

pub fn geo_stream_exe() -> PathBuf {
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
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../target")
        .join(profile)
        .join(name)
}

/// Run `geo-stream` with stdin bytes; capture full output.
#[allow(dead_code)]
pub fn run_geo_stream(stdin: &[u8]) -> Output {
    run_geo_stream_with_args(stdin, &[])
}

/// Run `geo-stream` with stdin bytes and additional CLI args; capture full output.
pub fn run_geo_stream_with_args(stdin: &[u8], args: &[&str]) -> Output {
    let bin = geo_stream_exe();
    assert!(
        bin.exists(),
        "geo-stream binary not found at {} (set CARGO_BIN_EXE_geo_stream or run `cargo test` from the workspace)",
        bin.display()
    );
    let mut child = Command::new(&bin)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn geo-stream");
    {
        let mut s = child.stdin.take().expect("stdin");
        s.write_all(stdin).expect("write stdin");
    }
    child.wait_with_output().expect("wait on geo-stream")
}

pub fn assert_success_empty_stderr(out: &Output) {
    assert!(
        out.status.success(),
        "geo-stream exited with {}; stderr: {}",
        out.status,
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        out.stderr.is_empty(),
        "expected empty stderr, got: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

pub fn stdout_event_lines(out: &Output) -> Vec<String> {
    let stdout = String::from_utf8(out.stdout.clone()).expect("utf-8 stdout");
    stdout
        .lines()
        .map(str::to_string)
        .filter(|l| !l.is_empty())
        .collect()
}
