# YT-Downloader RUST

## Requirements

- Linux / Mac - build with POSIX system paths in mind
- youtube-dl is installed and be accessable via the command `youtube-dl`
- ffmpeg is installed and be accessable via the command `ffmpeg`
- rust stable 1.40 or higher
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

### Change Archive path / disable archive

use `-r <ARCHIVE_FILE>` | `--archive <ARCHIVE_FILE>` (replace `ARCHIVE_FILE` with the path to the archive file, or `""`(empty) to disable archives)

Note: default archive file location `~/.config/yt-dl-rust.json`

### Ask for Edits

use `-e <BOOL>` | `--askedit <BOOL>` (replace `BOOL` with `true` or `false`)

With this option asking for edits after download can be enabled (by default) or disabled

Default: `true`

### Editor

use `--editor <EXECUTEABLE>` (replace `EXECUTEABLE` with the path / command to use as the editor)

With this option the editor to use can be set

Note: if empty, it will be asked after download

Default: ""

### Import already existing youtube-dl archive

use the subcommand `import <ARCHIVE_FILE>` (replace `ARCHIVE_FILE` with the path to the archive file)

Note: default archive file location `~/.config/yt-dl-rust.json`

---

Please note this project is still in development (so not finished) and im still new to rust
