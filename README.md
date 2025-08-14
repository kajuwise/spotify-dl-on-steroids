# spotify-dl

Improved command line utility to download songs, podcasts, playlists and albums directly from Spotify's servers.
You need a Spotify Premium account.

## Disclaimer

The usage of this software may infringe Spotify's ToS and/or your local legislation. For educational purposes only. Do not run in production servers.

## Installation from source
```
git clone https://github.com/kajuwise/spotify-dl-on-steroids.git
cd spotify-dl-on-steroids
cargo build --release
cargo install --path .
```

## Features in this fork

- Playlist sync feature - no need to enter url after first use. Playlist url information is cached in the folder. Just run `spotify-dl` and it will skip already downloaded songs and add only missing ones.
- Graceful handling of unavailable songs
- 320kbps mp3
- Album art and mp3 tags
- etc.

## Usage

```
spotify-dl
A commandline utility to download music directly from Spotify

USAGE:
    spotify-dl [OPTIONS]

FLAGS:
    -h, --help       Prints help information
    -V, --version    Prints version information

OPTIONAL:
    -f, --format <mp3 or flac>         Defining the output format, 320kbps mp3 by default
    -d, --destination <destination>    The directory where the songs will be downloaded
    -t, --turbo <parallel>             Turbo mode downloads songs in parallel. The number behind option
                                       defines the number of parallel threads: '-t 5' would download
                                       five songs simultaneously.
                                       In normal mode, the download speed is limited to mimic
                                       realistic streaming and there is varying delay between downloads.
    -r, --reset <reset>                Reset last-run-cache. Normally last run can be resumed in the same folder
                                       without specifying the track again. (playlist sync mode) 

OPTIONAL ARGS:
    <tracks>...    A list of Spotify URIs or URLs (songs, podcasts, playlists or albums). Automatically prompted if not provided.
```

Songs, playlists and albums must be passed as Spotify URIs or URLs (e.g. `spotify:track:123456789abcdefghABCDEF` for songs and `spotify:playlist:123456789abcdefghABCDEF` for playlists or `https://open.spotify.com/playlist/123456789abcdefghABCDEF?si=1234567890`).

## License

spotify-dl is licensed under the MIT license. See [LICENSE](LICENSE).
