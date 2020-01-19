# YT-Downloader RUST

## Requirements

- youtube-dl is installed and be accessable via the command `youtube-dl`
- ffmpeg is installed and be accessable via the command `ffmpeg`
- rust stable 1.40 or higher is installed
- having `cargo-make` installed for extra scripts [clippy, etc - build would still work without it]

## Usage

`./yt-downloader <URL>` (replace `<URL>` with the URL)

### audio only

add `-a` to make the output audio-only

### Print youtube-dl output

add `-d` to print stdout of `youtube-dl`

### Change Temporary Directory

add `--tmp <DIR>` (replace `<DIR>` with an absolute path to the directory)
default `/tmp`

### Create Sub-Directory in the Temporary Directory

add `-c <BOOL>` | `--tmpc <BOOL>` (replace `<BOOL>` with `true` or `false`)

### Extra youtube-dl Arguments

add `--` to the end of the command and every argument after that will be send to youtube-dl

### More Help

use `-h` | `--help`

---

Please note this project is still in development (so not finished) and im still new to rust
