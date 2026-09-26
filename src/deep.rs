use std::fs;
use std::future::Future;
use std::io::{self, BufRead, Write};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use librespot::core::SpotifyUri;
use spotify_dl::track::Track;

use crate::last_run_cache::{LAST_RUN_CACHE_PATH, LastRunCache};

const CONFIG_PATH: &str = ".spotify-dl-config.json";

#[derive(serde::Serialize, serde::Deserialize, Default)]
struct FolderConfig {
    #[serde(default, rename = "sub-directories")]
    subdirectories: bool,
}

pub fn is_enabled(root: &Path) -> Result<bool> {
    match fs::read_to_string(root.join(CONFIG_PATH)) {
        Ok(data) => {
            let config: FolderConfig = serde_json::from_str(&data).with_context(|| {
                format!("Invalid {CONFIG_PATH}; repair it or run spotify-dl -r")
            })?;
            Ok(config.subdirectories)
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error).with_context(|| format!("Cannot read {CONFIG_PATH}")),
    }
}

pub fn remember(root: &Path) -> Result<()> {
    fs::write(
        root.join(CONFIG_PATH),
        serde_json::to_string_pretty(&FolderConfig {
            subdirectories: true,
        })?,
    )
    .with_context(|| format!("Cannot save {CONFIG_PATH}"))
}

pub fn clear_preference(root: &Path) -> Result<()> {
    match fs::remove_file(root.join(CONFIG_PATH)) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error).with_context(|| format!("Cannot remove {CONFIG_PATH}")),
    }
}

#[derive(Debug)]
enum Status {
    Configured(Vec<String>),
    Missing,
    Invalid(String),
    Skipped(String),
}

#[derive(Debug)]
struct Entry {
    path: PathBuf,
    depth: usize,
    status: Status,
    scan_errors: Vec<String>,
}

pub struct Scan {
    entries: Vec<Entry>,
}

#[derive(Debug)]
pub struct Job {
    pub path: PathBuf,
    pub urls: Vec<String>,
}

fn configuration(path: &Path) -> Status {
    let data = match fs::read_to_string(path.join(LAST_RUN_CACHE_PATH)) {
        Ok(data) => data,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Status::Missing,
        Err(error) => return Status::Invalid(error.to_string()),
    };
    let cache = match serde_json::from_str::<LastRunCache>(&data) {
        Ok(cache) => cache,
        Err(error) => return Status::Invalid(format!("cannot read saved URLs: {error}")),
    };
    if cache.url.is_empty() {
        return Status::Invalid("no saved URLs".into());
    }
    for (index, url) in cache.url.iter().enumerate() {
        let supported = Track::new(url).is_ok_and(|track| {
            matches!(
                track.id,
                SpotifyUri::Track { .. }
                    | SpotifyUri::Episode { .. }
                    | SpotifyUri::Album { .. }
                    | SpotifyUri::Playlist { .. }
            )
        });
        if !supported {
            return Status::Invalid(format!("saved URL {} is invalid or unsupported", index + 1));
        }
    }
    Status::Configured(cache.url)
}

pub fn scan(root: &Path) -> Result<Scan> {
    let root = fs::canonicalize(root).context("Cannot resolve the starting folder")?;
    anyhow::ensure!(root.is_dir(), "Starting path must be a directory");
    let mut pending = vec![(root, 0)];
    let mut entries = Vec::new();
    while let Some((path, depth)) = pending.pop() {
        let mut entry = Entry {
            path,
            depth,
            status: Status::Missing,
            scan_errors: Vec::new(),
        };
        match fs::symlink_metadata(&entry.path) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                entry.status = Status::Skipped("symbolic link; not followed".into());
                entries.push(entry);
                continue;
            }
            Ok(metadata) if metadata.is_dir() => {}
            Ok(_) => continue,
            Err(error) => {
                entry.status = Status::Skipped(format!("cannot inspect path: {error}"));
                entries.push(entry);
                continue;
            }
        }
        entry.status = configuration(&entry.path);
        let mut children = Vec::new();
        match fs::read_dir(&entry.path) {
            Ok(directory) => {
                for child in directory {
                    match child {
                        Ok(child) => match child.file_type() {
                            Ok(kind) if kind.is_dir() || kind.is_symlink() => {
                                children.push(child.path());
                            }
                            Ok(_) => {}
                            Err(error) => entry.scan_errors.push(format!(
                                "cannot inspect {}: {error}",
                                child.path().display()
                            )),
                        },
                        Err(error) => entry.scan_errors.push(error.to_string()),
                    }
                }
            }
            Err(error) => entry.scan_errors.push(error.to_string()),
        }
        children.sort();
        pending.extend(children.into_iter().rev().map(|path| (path, depth + 1)));
        entries.push(entry);
    }
    Ok(Scan { entries })
}

impl Scan {
    pub fn jobs(&self) -> Vec<Job> {
        self.entries
            .iter()
            .filter_map(|entry| match &entry.status {
                Status::Configured(urls) => Some(Job {
                    path: entry.path.clone(),
                    urls: urls.clone(),
                }),
                _ => None,
            })
            .collect()
    }

    pub fn skipped_count(&self) -> usize {
        self.entries
            .iter()
            .filter(|entry| !matches!(entry.status, Status::Configured(_)))
            .count()
    }

    pub fn confirm(&self, input: &mut impl BufRead, output: &mut impl Write) -> Result<bool> {
        writeln!(
            output,
            "Folder sync preview (including the starting folder):"
        )?;
        for entry in &self.entries {
            let label = if entry.depth == 0 {
                entry.path.display().to_string()
            } else {
                entry
                    .path
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .into_owned()
            };
            let status = match &entry.status {
                Status::Configured(_) => "Will sync".into(),
                Status::Missing => "Not configured".into(),
                Status::Invalid(reason) => format!("Invalid configuration: {reason}"),
                Status::Skipped(reason) => format!("Skipped: {reason}"),
            };
            writeln!(output, "{}{} [{}]", "  ".repeat(entry.depth), label, status)?;
            for error in &entry.scan_errors {
                writeln!(
                    output,
                    "{}WARNING: scan incomplete: {}",
                    "  ".repeat(entry.depth + 1),
                    error
                )?;
            }
        }
        let count = self.entries.len() - self.skipped_count();
        writeln!(
            output,
            "\n{count} configured; {} skipped.",
            self.skipped_count()
        )?;
        writeln!(
            output,
            "Run spotify-dl manually inside each unconfigured folder to set it up."
        )?;
        writeln!(
            output,
            "For invalid configuration, repair .last_run_cache.dl or run spotify-dl -r in that folder."
        )?;
        if count == 0 {
            writeln!(output, "No configured folders to sync.")?;
            return Ok(false);
        }
        write!(output, "\nSync these {count} configured folders? [y/N] ")?;
        output.flush()?;
        let mut answer = String::new();
        input.read_line(&mut answer)?;
        let confirmed = matches!(answer.trim().to_ascii_lowercase().as_str(), "y" | "yes");
        if !confirmed {
            writeln!(output, "Cancelled. No folders were synced.")?;
        }
        Ok(confirmed)
    }
}

#[derive(Default)]
pub struct Summary {
    completed: Vec<PathBuf>,
    failed: Vec<(PathBuf, String)>,
}

impl Summary {
    pub fn print(&self, skipped: usize, output: &mut impl Write) -> Result<()> {
        writeln!(
            output,
            "\nFolder runs: {} completed, {} failed, {skipped} skipped.",
            self.completed.len(),
            self.failed.len()
        )?;
        for path in &self.completed {
            writeln!(output, "Completed: {}", path.display())?;
        }
        for (path, error) in &self.failed {
            writeln!(output, "Failed: {}: {error}", path.display())?;
        }
        Ok(())
    }

    pub fn into_result(self) -> Result<()> {
        anyhow::ensure!(
            self.failed.is_empty(),
            "{} folder run(s) failed",
            self.failed.len()
        );
        Ok(())
    }
}

pub async fn run_batch<F, Fut>(
    jobs: Vec<Job>,
    output: &mut impl Write,
    mut run: F,
) -> Result<Summary>
where
    F: FnMut(Job) -> Fut,
    Fut: Future<Output = Result<()>>,
{
    let mut summary = Summary::default();
    let total = jobs.len();
    for (index, job) in jobs.into_iter().enumerate() {
        let path = job.path.clone();
        writeln!(
            output,
            "\n[{}/{total}] Syncing {}",
            index + 1,
            path.display()
        )?;
        output.flush()?;
        match run(job).await {
            Ok(()) => summary.completed.push(path),
            Err(error) => {
                writeln!(output, "Folder failed: {}: {error:#}", path.display())?;
                summary.failed.push((path, format!("{error:#}")));
            }
        }
    }
    Ok(summary)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    const URL: &str = "spotify:playlist:37i9dQZF1DXcBWIGoYBM5M";
    static NEXT_DIR: AtomicUsize = AtomicUsize::new(0);

    struct TestDir(PathBuf);

    impl TestDir {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!(
                "spotify-dl-unit-{}-{}",
                std::process::id(),
                NEXT_DIR.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir(&path).unwrap();
            Self(fs::canonicalize(path).unwrap())
        }

        fn folder(&self, name: &str, cache: Option<&str>) -> PathBuf {
            let path = self.0.join(name);
            fs::create_dir_all(&path).unwrap();
            if let Some(cache) = cache {
                fs::write(path.join(LAST_RUN_CACHE_PATH), cache).unwrap();
            }
            path
        }

        fn configured(&self, name: &str) -> PathBuf {
            self.folder(name, Some(&format!(r#"{{"url":["{URL}"]}}"#)))
        }
    }

    impl Drop for TestDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn scans_root_hidden_and_nested_folders_in_sorted_order() {
        let dir = TestDir::new();
        let root = dir.configured("");
        let nested = dir.configured("b parent/child ü");
        let hidden = dir.configured(".hidden");
        let first = dir.configured("a");
        let history_only = dir.folder("history only", None);
        fs::write(history_only.join(".spotify-dl-history.json"), "{}").unwrap();
        let scan = scan(&dir.0).unwrap();
        let paths: Vec<_> = scan.jobs().into_iter().map(|job| job.path).collect();
        assert_eq!(paths, [root, hidden, first, nested]);
        assert_eq!(scan.skipped_count(), 2);
        let mut preview = Vec::new();
        assert!(scan.confirm(&mut &b"yes\n"[..], &mut preview).unwrap());
        let preview = String::from_utf8(preview).unwrap();
        assert!(preview.contains("  b parent [Not configured]\n    child ü [Will sync]"));
        assert!(preview.contains("history only [Not configured]"));
    }

    #[test]
    fn invalid_configs_are_skipped_and_preserved() {
        let dir = TestDir::new();
        let configs = [
            "",
            "not json",
            r#"{"url":[]}"#,
            r#"{"url":[""]}"#,
            r#"{"url":["spotify:artist:0OdUWJ0sBjDrqHygGUXeCF"]}"#,
            r#"{"url":["https://example.com/playlist/123"]}"#,
            r#"{"url":null}"#,
        ];
        for (index, cache) in configs.iter().enumerate() {
            dir.folder(&index.to_string(), Some(cache));
        }
        let unreadable = dir.folder("unreadable", None);
        fs::create_dir(unreadable.join(LAST_RUN_CACHE_PATH)).unwrap();
        let scan = scan(&dir.0).unwrap();
        assert!(scan.jobs().is_empty());
        assert_eq!(
            scan.entries
                .iter()
                .filter(|e| matches!(e.status, Status::Invalid(_)))
                .count(),
            8
        );
        for (index, cache) in configs.iter().enumerate() {
            assert_eq!(
                fs::read_to_string(dir.0.join(index.to_string()).join(LAST_RUN_CACHE_PATH))
                    .unwrap(),
                *cache
            );
        }
    }

    #[test]
    fn accepts_supported_urls_and_uris_and_rejects_mixed_invalid_entries() {
        let dir = TestDir::new();
        let urls = [
            URL,
            "spotify:track:4uLU6hMCjMI75M1A2tKUQC",
            "spotify:album:4uLU6hMCjMI75M1A2tKUQC",
            "spotify:episode:4uLU6hMCjMI75M1A2tKUQC",
            "https://open.spotify.com/playlist/37i9dQZF1DXcBWIGoYBM5M?si=abc",
            "https://open.spotify.com/intl-et/album/4uLU6hMCjMI75M1A2tKUQC",
        ];
        dir.folder("valid", Some(&serde_json::json!({"url": urls}).to_string()));
        dir.folder(
            "mixed",
            Some(&serde_json::json!({"url": [URL, "bad"]}).to_string()),
        );
        let jobs = scan(&dir.0).unwrap().jobs();
        assert_eq!(jobs.len(), 1);
        assert_eq!(jobs[0].urls, urls);
    }

    #[test]
    fn confirmation_is_explicit_and_empty_batches_do_not_prompt() {
        let dir = TestDir::new();
        let mut output = Vec::new();
        assert!(
            !scan(&dir.0)
                .unwrap()
                .confirm(&mut &b"yes\n"[..], &mut output)
                .unwrap()
        );
        assert!(!String::from_utf8(output).unwrap().contains("[y/N]"));
        dir.configured("");
        let scan = scan(&dir.0).unwrap();
        for answer in ["", "\n", "no\n", "sure\n", "yes please\n"] {
            assert!(
                !scan
                    .confirm(&mut answer.as_bytes(), &mut Vec::new())
                    .unwrap()
            );
        }
        for answer in ["y\n", "YES\n", " Yes \n"] {
            assert!(
                scan.confirm(&mut answer.as_bytes(), &mut Vec::new())
                    .unwrap()
            );
        }
    }

    #[test]
    fn preference_is_separate_from_urls_and_can_be_reset() {
        let dir = TestDir::new();
        dir.configured("");
        let original = fs::read(dir.0.join(LAST_RUN_CACHE_PATH)).unwrap();
        assert!(!is_enabled(&dir.0).unwrap());
        remember(&dir.0).unwrap();
        assert!(is_enabled(&dir.0).unwrap());
        let data: serde_json::Value =
            serde_json::from_slice(&fs::read(dir.0.join(CONFIG_PATH)).unwrap()).unwrap();
        assert_eq!(data["sub-directories"], true);
        clear_preference(&dir.0).unwrap();
        clear_preference(&dir.0).unwrap();
        assert!(!is_enabled(&dir.0).unwrap());
        assert_eq!(fs::read(dir.0.join(LAST_RUN_CACHE_PATH)).unwrap(), original);
        fs::write(dir.0.join(CONFIG_PATH), r#"{"sub-directories":false}"#).unwrap();
        assert!(!is_enabled(&dir.0).unwrap());
        fs::write(dir.0.join(CONFIG_PATH), "broken").unwrap();
        assert!(is_enabled(&dir.0).is_err());
    }

    #[tokio::test]
    async fn batch_is_sequential_continues_after_failure_and_keeps_confirmed_urls() {
        let dir = TestDir::new();
        let a = dir.configured("a");
        let b = dir.configured("b");
        let c = dir.configured("c");
        let jobs = scan(&dir.0).unwrap().jobs();
        fs::write(a.join(LAST_RUN_CACHE_PATH), "changed after preview").unwrap();
        let events = std::sync::Mutex::new(Vec::new());
        let summary = run_batch(jobs, &mut Vec::new(), |job| {
            events.lock().unwrap().push(("start", job.path.clone()));
            let events = &events;
            async move {
                assert_eq!(job.urls, [URL]);
                tokio::task::yield_now().await;
                events.lock().unwrap().push(("end", job.path.clone()));
                anyhow::ensure!(job.path.file_name().unwrap() != "b", "simulated failure");
                Ok(())
            }
        })
        .await
        .unwrap();
        assert_eq!(
            *events.lock().unwrap(),
            [
                ("start", a.clone()),
                ("end", a.clone()),
                ("start", b.clone()),
                ("end", b.clone()),
                ("start", c.clone()),
                ("end", c.clone()),
            ]
        );
        assert_eq!(summary.completed, [a, c]);
        assert_eq!(summary.failed[0].0, b);
        let mut output = Vec::new();
        summary.print(1, &mut output).unwrap();
        assert!(
            String::from_utf8(output)
                .unwrap()
                .contains("2 completed, 1 failed, 1 skipped")
        );
        assert!(summary.into_result().is_err());
        assert!(
            run_batch(Vec::new(), &mut Vec::new(), |_| async { Ok(()) })
                .await
                .unwrap()
                .into_result()
                .is_ok()
        );
    }

    #[cfg(unix)]
    #[test]
    fn skips_directory_symlinks_including_loops() {
        let dir = TestDir::new();
        dir.configured("child");
        std::os::unix::fs::symlink(&dir.0, dir.0.join("child/loop")).unwrap();
        std::os::unix::fs::symlink(dir.0.join("child"), dir.0.join("alias")).unwrap();
        let scan = scan(&dir.0).unwrap();
        assert_eq!(scan.jobs().len(), 1);
        assert_eq!(
            scan.entries
                .iter()
                .filter(|e| matches!(e.status, Status::Skipped(_)))
                .count(),
            2
        );
    }

    #[cfg(unix)]
    #[test]
    fn unreadable_branch_is_reported() {
        use std::os::unix::fs::PermissionsExt;
        let dir = TestDir::new();
        let locked = dir.folder("locked", None);
        fs::set_permissions(&locked, fs::Permissions::from_mode(0o000)).unwrap();
        let inaccessible = fs::read_dir(&locked).is_err();
        let scan = scan(&dir.0).unwrap();
        fs::set_permissions(&locked, fs::Permissions::from_mode(0o700)).unwrap();
        // Root can still read mode-000 directories.
        if inaccessible {
            let mut output = Vec::new();
            scan.confirm(&mut &b""[..], &mut output).unwrap();
            assert!(
                String::from_utf8(output)
                    .unwrap()
                    .contains("scan incomplete")
            );
        }
    }
}
