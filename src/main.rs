use librespot::core::session::Session;
use spotify_dl::download::{DownloadOptions, Downloader};
use spotify_dl::encoder::Format;
use spotify_dl::history::PlaylistHistory;
use spotify_dl::log;
use spotify_dl::session::create_session;
use spotify_dl::track::get_tracks;
use std::fs;
use std::fs::File;
use std::io::{self, Write};
use std::sync::Arc;
use structopt::StructOpt;
use tokio::sync::Mutex;

mod deep;
mod last_run_cache;
use last_run_cache::{LAST_RUN_CACHE_PATH, LastRunCache};

#[derive(Debug, StructOpt)]
#[structopt(
    name = "spotify-dl",
    about = "A commandline utility to download music directly from Spotify"
)]
struct Opt {
    #[structopt(
        short = "s",
        help = "Preview and sync this folder and all subfolders; remember this mode after confirmation",
        conflicts_with_all = &["tracks", "destination", "reset"]
    )]
    subfolders: bool,
    #[structopt(help = "A list of Spotify URIs or URLs (songs, podcasts, playlists or albums)")]
    tracks: Vec<String>,
    #[structopt(
        short = "d",
        long = "destination",
        help = "The directory where the songs will be downloaded"
    )]
    destination: Option<String>,
    #[structopt(
        short = "t",
        long = "turbo",
        alias = "parallel",
        help = "Turbo mode downloads songs in parallel (e.g. '-t 5' downloads five songs simultaneously).\nIn normal mode the download speed mimics Spotify streaming with delays between songs.",
        default_value = "1"
    )]
    parallel: usize,
    #[structopt(
        short = "f",
        long = "format",
        help = "The format to download the tracks in. Default is mp3 (320kbps).",
        default_value = "mp3"
    )]
    format: Format,
    #[structopt(
        short,
        long,
        help = "Reset saved URLs and remembered subfolder mode in this folder"
    )]
    reset: bool,
    #[structopt(
        short = "F",
        long = "force",
        help = "Force download even if the file already exists"
    )]
    force: bool,
}

impl Opt {
    fn uses_subfolders(&self, root: &std::path::Path) -> anyhow::Result<bool> {
        Ok(self.subfolders
            || (!self.reset
                && self.tracks.is_empty()
                && self.destination.is_none()
                && deep::is_enabled(root)?))
    }
}

pub fn create_destination_if_required(destination: Option<String>) -> anyhow::Result<()> {
    if let Some(destination) = destination {
        if !std::path::Path::new(&destination).exists() {
            tracing::info!("Creating destination directory: {}", destination);
            std::fs::create_dir_all(destination)?;
        }
    }
    Ok(())
}
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let mut opt = Opt::from_args();
    let root = std::env::current_dir()?;
    if opt.uses_subfolders(&root)? {
        return sync_subfolders(&opt).await;
    }
    log::configure_logger()?;
    create_destination_if_required(opt.destination.clone())?;

    let last_run_cache_path = LAST_RUN_CACHE_PATH;

    if opt.reset {
        deep::clear_preference(&root)?;
        match fs::remove_file(last_run_cache_path) {
            Ok(_) => println!(
                "Reset mode! Erased last run cache file: {}",
                last_run_cache_path
            ),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e.into()),
        }
    }

    use_last_run_cache_if_applicable(&mut opt, last_run_cache_path)?;
    prompt_track_if_necessary(&mut opt);
    store_last_run_cache(&opt, last_run_cache_path)?;

    let session = create_session().await?;

    let download_options =
        DownloadOptions::new(opt.destination, opt.parallel, opt.format, opt.force);
    download_folder(opt.tracks, session, download_options).await
}

async fn sync_subfolders(opt: &Opt) -> anyhow::Result<()> {
    anyhow::ensure!(
        opt.parallel > 0,
        "Turbo parallelism must be greater than zero"
    );
    let scan = deep::scan(&std::env::current_dir()?)?;
    let confirmed = {
        let stdin = io::stdin();
        let stdout = io::stdout();
        scan.confirm(&mut stdin.lock(), &mut stdout.lock())?
    };
    if !confirmed {
        return Ok(());
    }

    if opt.subfolders {
        deep::remember(&std::env::current_dir()?)?;
        println!("Subfolder mode saved. Next time, run spotify-dl here without -s.");
    }
    log::configure_logger()?;
    let session = create_session().await?;
    let summary = deep::run_batch(scan.jobs(), &mut io::stdout(), |job| {
        let options = DownloadOptions {
            destination: job.path,
            parallel: opt.parallel,
            format: opt.format,
            force: opt.force,
        };
        download_folder(job.urls, session.clone(), options)
    })
    .await?;
    summary.print(scan.skipped_count(), &mut io::stdout())?;
    summary.into_result()
}

async fn download_folder(
    urls: Vec<String>,
    session: Session,
    download_options: DownloadOptions,
) -> anyhow::Result<()> {
    let mut tracks = get_tracks(urls, &session).await?;

    let history = if tracks.iter().any(|track| track.playlist().is_some()) {
        let history_path = download_options
            .destination
            .join(".spotify-dl-history.json");
        let history = PlaylistHistory::load(history_path);

        if !download_options.force {
            let total_before = tracks.len();
            tracks = tracks
                .into_iter()
                .filter(|track| {
                    if let Some(playlist) = track.playlist() {
                        if history.has_downloaded(&playlist, &track.id) {
                            println!(
                                "Skipping track {} - already downloaded from playlist history",
                                track.id
                            );
                            return false;
                        }
                    }
                    true
                })
                .collect();

            let skipped = total_before.saturating_sub(tracks.len());
            if skipped > 0 {
                println!(
                    "Playlist history matched {skipped} tracks. Skipping metadata fetch for them."
                );
            }
        }
        Some(Arc::new(Mutex::new(history)))
    } else {
        None
    };

    let downloader = Downloader::new(session, history);
    downloader.download_tracks(tracks, &download_options).await
}

fn store_last_run_cache(opt: &Opt, last_run_cache_path: &str) -> anyhow::Result<()> {
    let last_run_cache = LastRunCache {
        url: opt.tracks.clone(),
    };
    let cache_json = serde_json::to_string_pretty(&last_run_cache)?;
    File::create(last_run_cache_path)?.write_all(cache_json.as_bytes())?;
    Ok(())
}

fn use_last_run_cache_if_applicable(
    opt: &mut Opt,
    last_run_cache_path: &str,
) -> anyhow::Result<()> {
    if opt.tracks.is_empty() && !opt.reset {
        match fs::read_to_string(last_run_cache_path) {
            Ok(data) => {
                if !data.trim().is_empty() {
                    println!("Tracks not provided.");
                    println!(
                        "Found last run cache. Will run in folder sync-mode with same tracks as last time:"
                    );
                    match serde_json::from_str::<LastRunCache>(&data) {
                        Ok(last_run_cache) if !last_run_cache.url.is_empty() => {
                            println!("{}", last_run_cache.url.join(", "));
                            println!(
                                "(Tip: Run with flag -r to clear folder sync-mode state or specify a different track via command argument.)\n"
                            );
                            opt.tracks.extend(last_run_cache.url);
                        }
                        Ok(_) => {}
                        Err(_) => {
                            eprintln!(
                                "⚠️  Last run cache file corrupted. Erasing: {last_run_cache_path}"
                            );
                            let _ = fs::remove_file(last_run_cache_path);
                        }
                    }
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e.into()),
        }
    }
    Ok(())
}

fn prompt_track_if_necessary(opt: &mut Opt) {
    if opt.tracks.is_empty() {
        print!("Enter a Spotify URL or URI: ");
        io::stdout().flush().unwrap();
        let mut input = String::new();
        io::stdin().read_line(&mut input).unwrap();
        let input = input.trim();
        if input.is_empty() {
            eprintln!("No tracks provided");
            std::process::exit(1);
        }
        opt.tracks.push(input.to_string());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ordinary_saved_urls_and_explicit_options_keep_single_folder_behavior() {
        let root = std::env::temp_dir().join(format!("spotify-dl-ordinary-{}", std::process::id()));
        fs::create_dir(&root).unwrap();
        let result = (|| -> anyhow::Result<()> {
            let cache_path = root.join(LAST_RUN_CACHE_PATH);
            let cache_path = cache_path.to_str().unwrap();
            let saved =
                Opt::from_iter_safe(["spotify-dl", "spotify:playlist:37i9dQZF1DXcBWIGoYBM5M"])?;
            store_last_run_cache(&saved, cache_path)?;

            let mut plain = Opt::from_iter_safe(["spotify-dl"])?;
            assert!(!plain.uses_subfolders(&root)?);
            use_last_run_cache_if_applicable(&mut plain, cache_path)?;
            assert_eq!(plain.tracks, saved.tracks);

            deep::remember(&root)?;
            assert!(Opt::from_iter_safe(["spotify-dl"])?.uses_subfolders(&root)?);
            for args in [
                vec!["spotify-dl", "-r"],
                vec!["spotify-dl", "-d", "out"],
                vec!["spotify-dl", "spotify:track:4uLU6hMCjMI75M1A2tKUQC"],
            ] {
                let mut opt = Opt::from_iter_safe(args)?;
                assert!(!opt.uses_subfolders(&root)?);
                let previous = opt.tracks.clone();
                use_last_run_cache_if_applicable(&mut opt, cache_path)?;
                if opt.reset || !previous.is_empty() {
                    assert_eq!(opt.tracks, previous);
                } else {
                    assert_eq!(opt.tracks, saved.tracks);
                }
                assert!(deep::is_enabled(&root)?);
            }
            Ok(())
        })();
        fs::remove_dir_all(root).unwrap();
        result.unwrap();
    }
}
