# spotify-dl-on-steroids

A fork of [spotify-dl](https://github.com/GuillemCastro/spotify-dl) project. 

Improved command line utility to download songs, podcasts, playlists and albums directly from Spotify's servers.
You need a Spotify Premium account. 
Tested on Mac and Windows.

## Disclaimer

The usage of this software may infringe Spotify's ToS and/or your local legislation. For educational purposes only. Do not run in production servers.

## Features in this fork

- Playlist sync feature - no need to enter url after first use. Playlist url information is cached in the folder. Just run `spotify-dl` again and it will skip already downloaded songs and add only missing ones.
- Deep directory mode - run `spotify-dl -s` once to preview and sync configured folders throughout a directory tree. After confirmation, the starting folder remembers this mode for future `spotify-dl` runs.
- Store download history in the folder. Skip already downloaded songs in playlist sync mode (not even fetching metadata)
- Graceful handling of unavailable songs
- 320kbps mp3 by default
- Album art and all available mp3 tags
- Mimic realistic streaming vs parallelized "turbo" mode
- etc.

## Latest changes and status
**September 2026 NEW! -s sub-directory mode.** Run the utility for all applicable sub-folders.
January 2025 - implement download history feature. Skip already downlaoded files. Speeds up playlist sync feature.<br>
November 2025 - pump to latest librespot library version and features.<br>

Tested 26.09.2026 (win), 26.09.2026 (mac) ✅<br>

## Installation from source
One liner:
```
cargo install --git https://github.com/kajuwise/spotify-dl-on-steroids --locked
```

Clone the repo
```
git clone https://github.com/kajuwise/spotify-dl-on-steroids.git
cd spotify-dl-on-steroids
```
Mac
```
cargo build --release
cargo install --path . --locked
```
Windows
```
cargo update -p vergen --precise 9.0.6
cargo build --release --locked
cargo install --path . --locked
```

In case you are missing Cargo:
Mac
```
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source "$HOME/.cargo/env"
```
## Usage

```
spotify-dl
A commandline utility to download music directly from Spotify

USAGE:
    spotify-dl [OPTIONS]

FLAGS:
    -h, --help       Prints help information
    -V, --version    Prints version information
    -s              Preview and sync this folder and all configured subfolders; remember this mode

OPTIONAL:
    -f, --format <mp3 or flac>         Defining the output format, 320kbps mp3 by default
    -d, --destination <destination>    The directory where the songs will be downloaded
    -t, --turbo <parallel>             Turbo mode downloads songs in parallel. The number behind option
                                       defines the number of parallel threads: '-t 5' would download
                                       five songs simultaneously.
                                       In normal mode, the download speed is limited to mimic
                                       realistic streaming and there is varying delay between downloads.
    -r, --reset                        Reset last-run-cache and remembered deep directory mode in this folder.
                                       Normally last run can be resumed in the same folder
                                       without specifying the track again. (playlist sync mode) 
    -F, --force                        Download again even if files or playlist history already exist

OPTIONAL ARGS:
    <tracks>...    A list of Spotify URIs or URLs (songs, podcasts, playlists or albums). Automatically prompted if not provided.
```

Songs, playlists and albums must be passed as Spotify URIs or URLs (e.g. `spotify:track:123456789abcdefghABCDEF` for songs and `spotify:playlist:123456789abcdefghABCDEF` for playlists or `https://open.spotify.com/playlist/123456789abcdefghABCDEF?si=1234567890`).

### Deep directory mode

From the parent folder containing your music folders, run:

```sh
spotify-dl -s
```

The program scans the starting folder and all descendants, including hidden directories. A folder is configured when its `.last_run_cache.dl` contains a nonempty list of supported Spotify URLs or URIs, saved by an earlier ordinary `spotify-dl` run. Download history alone does not configure a folder. Configured children are found even when their parents are not configured.

Before connecting to Spotify or downloading, it shows a tree like this:

```text
/Music [Not configured]
  Albums [Not configured]
    Favourites [Will sync]
  New music [Not configured]
  Playlists [Will sync]
    Discoveries [Will sync]

3 configured; 3 skipped.
Run spotify-dl manually inside each unconfigured folder to set it up.
For invalid configuration, repair .last_run_cache.dl or run spotify-dl -r in that folder.

Sync these 3 configured folders? [y/N]
```

Only `y` or `yes` (case insensitive) starts the batch. Pressing Enter, answering anything else, or reaching end of input cancels without changing folder configuration. Empty batches exit without prompting. Invalid or unreadable caches are listed and skipped without being modified; unreadable directory branches are marked as incomplete scans. Symbolic links are listed as skipped and are not followed.

After confirmation, the starting folder saves `.spotify-dl-config.json`:

```json
{
  "sub-directories": true
}
```

Next time, simply run `spotify-dl` from that same folder. It scans again and asks for confirmation every time. This preference is separate from saved playlist URLs and does not configure an unconfigured folder for downloads.

Folders run one by one in sorted tree order, using each folder's saved URLs, local download history, and destination. A folder failure does not stop the remaining folders; the final summary lists completed and failed folder runs, and any failed run results in a nonzero exit status. Existing handling of unavailable individual tracks still applies, so a completed folder run does not guarantee every track was available. Authentication failure ends the batch.

Use `-f`, `-t`, and `-F` to apply download options across the batch, for example `spotify-dl -s -t 3`. Turbo mode parallelizes tracks within a folder; folders always run sequentially. Explicit `-s` cannot be combined with track arguments, `-d`, or `-r`. When the mode is remembered, explicit track arguments or `-d` use ordinary single-folder behavior for that invocation and leave the preference saved.

To disable remembered deep directory mode while keeping saved playlist URLs, set `"sub-directories"` to `false` in `.spotify-dl-config.json` or remove that file. Running `spotify-dl -r` clears both this preference and the starting folder's saved URLs, then prompts for a new Spotify URL; it does not reset child folders. To configure a missing child folder, enter it and run `spotify-dl` manually.

## License

spotify-dl is licensed under the MIT license. See [LICENSE](LICENSE).
