#!/bin/sh

# This File is for invoking clippy::pedantic, but with some lints ignored that will not be fixed or a heavily false-positive

cargo clippy --all-features "$@" -- \
-W clippy::pedantic \
-A clippy::doc_markdown \
-A clippy::module_name_repetitions \
-A clippy::uninlined_format_args
