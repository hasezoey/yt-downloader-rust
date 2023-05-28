#![allow(clippy::implicit_return)]
// @generated automatically by Diesel CLI.

diesel::table! {
	media_archive (_id) {
		_id -> Integer,
		media_id -> Text,
		provider -> Text,
		title -> Text,
		inserted_at -> Timestamp,
	}
}
