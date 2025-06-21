use spotify_dl::download::{DownloadOptions, Downloader};
use spotify_dl::encoder::Format;
use spotify_dl::session::create_session;
use spotify_dl::track::get_tracks;
use structopt::StructOpt;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::{fmt, EnvFilter};
use std::io::{self, Error, Write};
use std::fs;
use std::fs::File;

mod last_run_cache;
use last_run_cache::LastRunCache;

#[derive(Debug, StructOpt)]
#[structopt(
    name = "spotify-dl",
    about = "A commandline utility to download music directly from Spotify"
)]
struct Opt {
    #[structopt(
        help = "A list of Spotify URIs or URLs (songs, podcasts, playlists or albums)"
    )]
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
        help = "Turbo mode downloads songs in parallel. '-t 5' would download five songs simultaneously.\n
        In normal mode, the download speed is limited to mimic realistic streaming and there is varying delay between downloads.",
        default_value = "1"
    )]
    parallel: usize,
    #[structopt(
        short = "f",
        long = "format",
        help = "The format to download the tracks in. Default is mp3. (320kbps, max 20kHz equal to top Spotify quality)",
        default_value = "mp3"
    )]
    format: Format,
    #[structopt(short, long, help = "Reset last run cache")]
    reset: bool
}

pub fn configure_logger() {
    tracing_subscriber::registry()
        .with(fmt::layer())
        .with(EnvFilter::from_default_env())
        .init();
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
    configure_logger();

    let mut opt = Opt::from_args();
    create_destination_if_required(opt.destination.clone())?;

    let last_run_cache_path = ".last_run_cache.dl";

    if opt.reset {
        println!("Reset mode! Erasing last run cache file: {}", last_run_cache_path);
        fs::remove_file(last_run_cache_path)?;
    }

    use_last_run_cache_if_applicable(&mut opt, last_run_cache_path)?;
    prompt_track_if_necessary(&mut opt);
    store_last_run_cache(&mut opt, last_run_cache_path)?;

    let session = create_session().await?;
    let track = get_tracks(opt.tracks, &session).await?;

    let downloader = Downloader::new(session);
    downloader
        .download_tracks(
            track,
            &DownloadOptions::new(opt.destination, opt.parallel, opt.format),
        )
        .await
}

fn store_last_run_cache(opt: &mut Opt, last_run_cache_path: &str) -> Result<(), Error> {
    let last_run_cache = LastRunCache {
        url: opt.tracks.clone(),
    };
    let cache_json = serde_json::to_string_pretty(&last_run_cache)?;
    File::create(last_run_cache_path)?.write_all(cache_json.as_bytes())?;
    Ok(())
}

fn use_last_run_cache_if_applicable(opt: &mut Opt, last_run_cache_path: &str) -> Result<(), Error> {
    if opt.tracks.is_empty() && !opt.reset {
        let data = fs::read_to_string(last_run_cache_path)?;
        if !data.is_empty() {
            print!("Tracks not provided.\nFound last run cache. Will run in folder sync-mode with same tracks as last time: ");
            let last_run_cache: LastRunCache = serde_json::from_str(&data)?;
            println!("{}", last_run_cache.url.join(", "));
            println!("(Tip: Run with flag -r to clear folder sync-mode state or specify a different track via command argument.)\n");
            opt.tracks.extend(last_run_cache.url);
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
