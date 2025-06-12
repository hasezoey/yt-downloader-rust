# libYTDLR

A library to interact with `youtube-dl` / `yt-dlp` from rust, with custom archive support.

This library is mainly made for [`ytdlr`](https://github.com/hasezoey/yt-downloader-rust/tree/master/crates/ytdlr) binary, but can also be consumed standalone.

For build / run requirements please see [the project README](https://github.com/hasezoey/yt-downloader-rust/blob/master/README.md#requirements).

## Functions provided

### download

The main functionality: interacting with `yt-dlp`; downloading actual media.

Small example:

```rs
libytdlr::main::download::download_single(connection, &options, progress_callback, &mut result_vec)?;
```

For a full example see [`examples/simple`](https://github.com/hasezoey/yt-downloader-rust/tree/master/crates/libytdlr/examples/simple.rs).

### rethumbnail

Extra functionality to re-apply a thumbnail to a video or audio container:

Small example:

```rs
libytdlr::main::rethumbnail::re_thumbnail_with_tmp(&media_path, &image_path, &output_path)?;
```

For a full example see [`examples/rehtumbnail`](https://github.com/hasezoey/yt-downloader-rust/tree/master/crates/libytdlr/examples/rethumbnail.rs).

### archive interaction

The custom archive `libytdlr` uses is based on SQLite and provides full read & write ability.
It is recommended to only do reads from outside functions.

The main function that will be necessary to be called to make use of the archive is:

```rs
libytdlr::main::sql_utils::migrate_and_connect(&database_path, progress_callback)?;
// or without any format migration:
libytdlr::main::sql_utils::sqlite_connect(&database_path)?;
```
