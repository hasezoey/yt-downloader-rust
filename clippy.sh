#!/bin/sh

# this file is an shorthand for the command below
CLIPPY_DISABLE_DOCS_LINKS=1 cargo +nightly clippy --all-features -Z unstable-options "$@" -- -D clippy::correctness -W clippy::style -W clippy::complexity -W clippy::perf -A clippy::needless_return -D clippy::implicit_return -A clippy::needless_doctest_main -A clippy::tabs_in_doc_comments
