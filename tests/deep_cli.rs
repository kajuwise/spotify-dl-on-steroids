use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Output, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};

static NEXT_DIR: AtomicUsize = AtomicUsize::new(0);
const CACHE: &str = r#"{"url":["spotify:playlist:37i9dQZF1DXcBWIGoYBM5M"]}"#;

struct TestDir(PathBuf);

impl TestDir {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "spotify-dl-cli-{}-{}",
            std::process::id(),
            NEXT_DIR.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }

    fn run(&self, args: &[&str], input: &str) -> Output {
        let mut child = Command::new(env!("CARGO_BIN_EXE_spotify-dl"))
            .args(args)
            .current_dir(&self.0)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        child
            .stdin
            .take()
            .unwrap()
            .write_all(input.as_bytes())
            .unwrap();
        child.wait_with_output().unwrap()
    }
}

impl Drop for TestDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn explicit_mode_cancellation_preserves_all_folder_state() {
    let dir = TestDir::new();
    fs::create_dir(dir.0.join("child")).unwrap();
    fs::write(dir.0.join("child/.last_run_cache.dl"), CACHE).unwrap();
    for answer in ["no\n", ""] {
        let output = dir.run(&["-s"], answer);
        assert!(output.status.success(), "{:?}", output);
        let stdout = String::from_utf8(output.stdout).unwrap();
        assert!(stdout.contains("child [Will sync]"));
        assert!(stdout.contains("[y/N]"));
        assert!(stdout.contains("Cancelled"));
        assert_eq!(fs::read_dir(&dir.0).unwrap().count(), 1);
        assert_eq!(fs::read_dir(dir.0.join("child")).unwrap().count(), 1);
        assert_eq!(
            fs::read_to_string(dir.0.join("child/.last_run_cache.dl")).unwrap(),
            CACHE
        );
    }
}

#[test]
fn remembered_mode_works_without_s_and_still_requires_confirmation() {
    let dir = TestDir::new();
    fs::write(
        dir.0.join(".spotify-dl-config.json"),
        r#"{"sub-directories":true}"#,
    )
    .unwrap();
    fs::write(dir.0.join(".last_run_cache.dl"), CACHE).unwrap();
    let output = dir.run(&[], "\n");
    assert!(output.status.success(), "{:?}", output);
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("Sync these 1 configured folders? [y/N]"));
    assert!(stdout.contains("Cancelled"));
    assert_eq!(fs::read_dir(&dir.0).unwrap().count(), 2);
}

#[test]
fn empty_tree_does_not_prompt_or_create_state() {
    let dir = TestDir::new();
    let output = dir.run(&["-s"], "");
    assert!(output.status.success(), "{:?}", output);
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("No configured folders"));
    assert!(!stdout.contains("[y/N]"));
    assert_eq!(fs::read_dir(&dir.0).unwrap().count(), 0);
}

#[test]
fn cli_conflicts_are_rejected_before_any_folder_changes() {
    let dir = TestDir::new();
    for args in [
        vec!["-s", "-r"],
        vec!["-s", "-d", "out"],
        vec!["-s", "spotify:track:4uLU6hMCjMI75M1A2tKUQC"],
        vec!["-s", "-t", "0"],
    ] {
        let output = dir.run(&args, "");
        assert!(!output.status.success(), "{args:?}");
        assert_eq!(fs::read_dir(&dir.0).unwrap().count(), 0);
    }
}

#[test]
fn download_flags_are_accepted_in_explicit_and_remembered_mode() {
    let dir = TestDir::new();
    let output = dir.run(&["-s", "-f", "flac", "-t", "3", "-F"], "");
    assert!(output.status.success(), "{:?}", output);
    fs::write(
        dir.0.join(".spotify-dl-config.json"),
        r#"{"sub-directories":true}"#,
    )
    .unwrap();
    let output = dir.run(&["-f", "flac", "-t", "3", "-F"], "");
    assert!(output.status.success(), "{:?}", output);
    assert!(
        String::from_utf8(output.stdout)
            .unwrap()
            .contains("No configured folders")
    );
}

#[test]
fn malformed_preference_reports_recovery_command() {
    let dir = TestDir::new();
    fs::write(dir.0.join(".spotify-dl-config.json"), "broken").unwrap();
    let output = dir.run(&[], "");
    assert!(!output.status.success());
    assert!(
        String::from_utf8(output.stderr)
            .unwrap()
            .contains("spotify-dl -r")
    );
    assert_eq!(
        fs::read_to_string(dir.0.join(".spotify-dl-config.json")).unwrap(),
        "broken"
    );
}
