use std::fmt::Write;
use std::path::PathBuf;
use tokio::time::{timeout, Duration};

use anyhow::Result;
use futures::StreamExt;
use futures::TryStreamExt;
use indicatif::MultiProgress;
use indicatif::ProgressBar;
use indicatif::ProgressState;
use indicatif::ProgressStyle;
use librespot::core::session::Session;

use crate::encoder;
use crate::encoder::Format;
use crate::encoder::Samples;
use crate::stream::Stream;
use crate::stream::StreamEvent;
use crate::stream::StreamEventChannel;
use crate::track::Track;
use crate::track::TrackMetadata;

pub struct Downloader {
    session: Session,
    progress_bar: MultiProgress,
}

#[derive(Debug, Clone)]
pub struct DownloadOptions {
    pub destination: PathBuf,
    pub parallel: usize,
    pub format: Format,
    pub force: bool,
}

impl DownloadOptions {
    pub fn new(destination: Option<String>, parallel: usize, format: Format, force: bool) -> Self {
        let destination =
            destination.map_or_else(|| std::env::current_dir().unwrap(), PathBuf::from);
        DownloadOptions {
            destination,
            parallel,
            format,
            force,
        }
    }
}

impl Downloader {
    pub fn new(session: Session) -> Self {
        Downloader {
            session,
            progress_bar: MultiProgress::new(),
        }
    }

    pub async fn download_tracks(
        self,
        tracks: Vec<Track>,
        options: &DownloadOptions,
    ) -> Result<()> {
        futures::stream::iter(tracks)
            .map(|track| self.download_track(track, options))
            .buffer_unordered(options.parallel)
            .try_collect::<Vec<_>>()
            .await?;

        Ok(())
    }

    #[tracing::instrument(name = "download_track", skip(self))]
    async fn download_track(&self, track: Track, options: &DownloadOptions) -> Result<()> {
        let metadata = match track.metadata(&self.session).await {
            Ok(metadata) => metadata,
            Err(err) => {
                tracing::warn!(error = %err, "Skipping track because metadata could not be loaded");
                println!("Skipping track {:?}: {}", track.id, err);
                return Ok(());
            }
        };
        tracing::info!("Downloading track: {:?}", metadata.track_name);

        let file_stem = self.get_file_name(&metadata);
        let mut target_path = options.destination.join(&file_stem);
        target_path.set_extension(options.format.extension());

        if !options.force {
            if target_path.exists() {
                println!(
                    "File already exists, skipping: {}",
                    target_path.display()
                );
                return Ok(());
            }

            if let Some(legacy) = self.legacy_file_name(&metadata) {
                let mut legacy_path = options.destination.join(&legacy);
                legacy_path.set_extension(options.format.extension());
                if legacy_path.exists() {
                    println!(
                        "File already exists, skipping: {}",
                        legacy_path.display()
                    );
                    return Ok(());
                }
            }
        }

        let path = target_path
            .to_str()
            .ok_or(anyhow::anyhow!("Could not set the output path"))?
            .to_string();

        let pb = self.add_progress_bar(&metadata, &file_stem);

        let stream = Stream::new(self.session.clone());
        let channel = match stream.stream(track).await {
            Ok(channel) => channel,
            Err(e) => {
                self.fail_with_error(&pb, &file_stem, e.to_string());
                return Ok(());
            }
        };

        let samples = match self.buffer_track(channel, &pb, &file_stem).await {
            Ok(Some(samples)) => samples,
            Ok(None) => {
                tracing::warn!("Skipping {}, song download timed out", file_stem);
                pb.finish_with_message(format!("Skipped {}", file_stem));
                return Ok(());
            }
            Err(e) => {
                self.fail_with_error(&pb, &file_stem, e.to_string());
                return Ok(());
            }
        };

        tracing::info!("Encoding track: {}", file_stem);
        pb.set_message(format!("Encoding {}", file_stem));

        let encoder = crate::encoder::get_encoder(options.format);
        let stream = encoder.encode(samples).await?;

        pb.set_message(format!("Writing {}", file_stem));
        tracing::info!("Writing track: {:?} to file: {}", file_stem, &path);
        stream.write_to_file(&path).await?;

        let tags = metadata.tags().await?;
        encoder::tags::store_tags(path, &tags, options.format).await?;

        if options.parallel == 1 {
            let delay_before_next_download = (metadata.duration.max(0) as u64) / 5;
            pb.set_message(format!(
                "Downloaded {}. Delaying next song by {}s",
                file_stem,
                delay_before_next_download / 1000
            ));
            tokio::time::sleep(Duration::from_millis(delay_before_next_download)).await;
            pb.finish_with_message(format!("Completed {}", file_stem));
        } else {
            pb.finish_with_message(format!("Downloaded {}", file_stem));
        }
        Ok(())
    }

    fn add_progress_bar(&self, track: &TrackMetadata, label: &str) -> ProgressBar {
        let pb = self
            .progress_bar
            .add(ProgressBar::new(track.approx_size() as u64));
        pb.enable_steady_tick(Duration::from_millis(100));
        pb.set_style(ProgressStyle::with_template("{spinner:.green} {msg} [{elapsed_precise}] [{wide_bar:.cyan/blue}] {bytes}/{total_bytes} ({eta})")
            // Infallible
            .unwrap()
            .with_key("eta", |state: &ProgressState, w: &mut dyn Write| write!(w, "{:.1}s", state.eta().as_secs_f64()).unwrap())
            .progress_chars("#>-") );
        pb.set_message(label.to_string());
        pb
    }

    async fn buffer_track(
        &self,
        mut rx: StreamEventChannel,
        pb: &ProgressBar,
        label: &str,
    ) -> Result<Option<Samples>> {
        let mut samples = Vec::<i32>::new();
        let timeout_duration = Duration::from_secs(30);
        loop {
            match timeout(timeout_duration, rx.recv()).await {
                Ok(Some(event)) => match event {
                    StreamEvent::Write {
                        bytes,
                        total,
                        mut content,
                    } => {
                        tracing::trace!("Written {} bytes out of {}", bytes, total);
                        pb.set_position(bytes as u64);
                        samples.append(&mut content);
                    }
                    StreamEvent::Finished => {
                        tracing::info!("Finished downloading track");
                        break;
                    }
                    StreamEvent::Error(stream_error) => {
                        tracing::error!("Error while streaming track: {:?}", stream_error);
                        return Err(anyhow::anyhow!("Streaming error: {:?}", stream_error));
                    }
                    StreamEvent::Retry {
                        attempt,
                        max_attempts,
                    } => {
                        tracing::warn!(
                            "Retrying download, attempt {} of {}: {}",
                            attempt,
                            max_attempts,
                            label
                        );
                        pb.set_message(format!(
                            "Retrying ({}/{}) {}",
                            attempt,
                            max_attempts,
                            label
                        ));
                    }
                },
                Ok(None) => break,
                Err(_) => {
                    println!("Song download timed out. Skipping.");
                    return Ok(None);
                }
            }
        }
        Ok(Some(Samples {
            samples,
            ..Default::default()
        }))
    }

    fn fail_with_error<S>(&self, pb: &ProgressBar, name: &str, e: S)
    where
        S: Into<String>,
    {
        tracing::error!("Failed to download {}: {}", name, e.into());
        pb.finish_with_message(
            console::style(format!("Failed! {}", name))
                .red()
                .to_string(),
        );
    }

    fn get_file_name(&self, metadata: &TrackMetadata) -> String {
        if metadata.artists.len() > 3 {
            let artists_name = metadata
                .artists
                .iter()
                .take(3)
                .map(|artist| artist.name.clone())
                .collect::<Vec<String>>()
                .join(", ");
            return self.clean_file_name(format!(
                "{}, and others - {}",
                artists_name, metadata.track_name
            ));
        }

        let artists_name = metadata
            .artists
            .iter()
            .map(|artist| artist.name.clone())
            .collect::<Vec<String>>()
            .join(", ");
        self.clean_file_name(format!("{} - {}", artists_name, metadata.track_name))
    }

    fn legacy_file_name(&self, metadata: &TrackMetadata) -> Option<String> {
        if metadata.artists.len() > 3 {
            let artists_name = metadata
                .artists
                .iter()
                .take(3)
                .map(|artist| artist.name.clone())
                .collect::<Vec<String>>()
                .join(", ");
            return Some(self.clean_file_name(format!(
                "{}, ... - {}",
                artists_name, metadata.track_name
            )));
        }
        None
    }

    fn clean_file_name(&self, file_name: String) -> String {
        let invalid_chars = ['<', '>', ':', '\'', '"', '/', '\\', '|', '?', '*', '.'];
        let mut clean = String::new();

        let allows_non_ascii = !cfg!(windows);
        for c in file_name.chars() {
            if !invalid_chars.contains(&c) && (c.is_ascii() || allows_non_ascii) && !c.is_control()
            {
                clean.push(c);
            }
        }
        clean
    }
}
