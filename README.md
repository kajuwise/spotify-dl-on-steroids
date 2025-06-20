# spotify-dl

A command line utility to download songs, podcasts, playlists and albums directly from Spotify's servers.

You need a Spotify Premium account.

## Disclaimer

The usage of this software may infringe Spotify's ToS and/or your local legislation. For educational purposes only. Do not run in production servers.

## Installation from source
```
git clone https://github.com/GuillemCastro/spotify-dl.git
cd spotify-dl
cargo build --release
cargo install --path .
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

OPTIONAL:
    -f, --format <mp3 or flac>         Defining the output format, 320kbps mp3 by default
    -c, --compression <compression>    Setting the flac compression level from 0 (fastest, least compression) to
                                       8 (slowest, most compression). A value larger than 8 will be Treated as 8.
                                       Default is 4.
    -d, --destination <destination>    The directory where the songs will be downloaded
    -t, --parallel <parallel>          Number of parallel downloads. Default is 5. [default: 5]

OPTIONAL ARGS:
    <tracks>...    A list of Spotify URIs or URLs (songs, podcasts, playlists or albums). Automatically prompted if not provided.
```

Songs, playlists and albums must be passed as Spotify URIs or URLs (e.g. `spotify:track:123456789abcdefghABCDEF` for songs and `spotify:playlist:123456789abcdefghABCDEF` for playlists or `https://open.spotify.com/playlist/123456789abcdefghABCDEF?si=1234567890`).

## License

spotify-dl is licensed under the MIT license. See [LICENSE](LICENSE).
