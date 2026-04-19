use serde_json::Value;
use std::io::{BufRead, BufReader};
use std::process::{Command, Stdio};
use std::thread;
use std::time::Duration;
use tempfile::TempDir;

fn bin() -> String {
    env!("CARGO_BIN_EXE_tk").to_owned()
}

fn run_json(root: &TempDir, args: &[&str]) -> Value {
    run_json_with_status(root, args, Some(0))
}

fn run_json_with_status(root: &TempDir, args: &[&str], expected_status: Option<i32>) -> Value {
    let output = Command::new(bin())
        .args(["--root", root.path().to_str().unwrap(), "--format", "json"])
        .args(args)
        .output()
        .expect("command should run");
    if let Some(expected_status) = expected_status {
        assert_eq!(
            output.status.code(),
            Some(expected_status),
            "stderr: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON")
}

#[test]
fn create_claim_block_and_next_flow() {
    let root = TempDir::new().unwrap();

    let init = run_json(&root, &["init"]);
    assert_eq!(init["ok"], Value::Bool(true));

    let created_1 = run_json(&root, &["create", "Task one", "--description", "first"]);
    let created_2 = run_json(&root, &["create", "Task two", "--description", "second"]);
    assert_eq!(created_1["task"]["id"], "1");
    assert_eq!(created_2["task"]["id"], "2");

    let claimed = run_json(&root, &["claim", "1", "--owner", "codex", "--start"]);
    assert_eq!(claimed["task"]["owner"], "codex");
    assert_eq!(claimed["task"]["status"], "in_progress");

    let blocked = run_json(&root, &["block", "add", "2", "1"]);
    assert_eq!(blocked["task"]["open_blocked_by"][0], "1");

    let next_none = Command::new(bin())
        .args([
            "--root",
            root.path().to_str().unwrap(),
            "--format",
            "json",
            "next",
        ])
        .output()
        .expect("command should run");
    assert_eq!(next_none.status.code(), Some(3));

    let done = run_json(&root, &["done", "1"]);
    assert_eq!(done["task"]["status"], "completed");

    let next = run_json(&root, &["next"]);
    assert_eq!(next["task"]["id"], "2");
    assert_eq!(next["task"]["claimable"], true);
}

#[test]
fn repeated_claim_and_block_add_are_idempotent() {
    let root = TempDir::new().unwrap();
    run_json(&root, &["init"]);
    run_json(&root, &["create", "Task one"]);
    run_json(&root, &["create", "Task two"]);

    let first_claim = run_json(&root, &["claim", "1", "--owner", "codex"]);
    let second_claim = run_json(&root, &["claim", "1", "--owner", "codex"]);
    assert_eq!(
        first_claim["task"]["revision"],
        second_claim["task"]["revision"]
    );
    assert_eq!(
        first_claim["list"]["list_revision"],
        second_claim["list"]["list_revision"]
    );

    let first_block = run_json(&root, &["block", "add", "2", "1"]);
    let second_block = run_json(&root, &["block", "add", "2", "1"]);
    assert_eq!(
        first_block["task"]["revision"],
        second_block["task"]["revision"]
    );
    assert_eq!(
        first_block["list"]["list_revision"],
        second_block["list"]["list_revision"]
    );
}

#[test]
fn verify_reports_missing_blocker() {
    let root = TempDir::new().unwrap();
    run_json(&root, &["init"]);
    run_json(&root, &["create", "Task one"]);

    let task_path = root
        .path()
        .join("lists")
        .join("tk")
        .join("tasks")
        .join("1.json");
    let mut task: Value = serde_json::from_slice(&std::fs::read(&task_path).unwrap()).unwrap();
    task["blocked_by"] = Value::Array(vec![Value::String("999".to_owned())]);
    std::fs::write(&task_path, serde_json::to_vec_pretty(&task).unwrap()).unwrap();

    let verify = run_json_with_status(&root, &["verify"], Some(4));
    let diagnostics = verify["diagnostics"].as_array().unwrap();
    assert!(
        diagnostics
            .iter()
            .any(|diag| diag["code"] == "blocker_missing")
    );
}

#[test]
fn watch_emits_snapshot_and_create_event() {
    let root = TempDir::new().unwrap();

    let mut child = Command::new(bin())
        .args([
            "--root",
            root.path().to_str().unwrap(),
            "--format",
            "ndjson",
            "watch",
            "--interval-ms",
            "100",
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("watch should start");

    thread::sleep(Duration::from_millis(300));
    run_json(&root, &["init"]);
    run_json(&root, &["create", "Watched task"]);
    thread::sleep(Duration::from_millis(500));

    let stdout = child.stdout.take().unwrap();
    let reader = BufReader::new(stdout);
    let lines = reader
        .lines()
        .take(2)
        .map(|line| serde_json::from_str::<Value>(&line.unwrap()).unwrap())
        .collect::<Vec<_>>();

    let _ = child.kill();
    let _ = child.wait();

    assert_eq!(lines[0]["type"], "snapshot");
    assert_eq!(lines[1]["type"], "task_created");
    assert_eq!(lines[1]["task"]["subject"], "Watched task");
}
