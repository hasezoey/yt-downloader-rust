#![allow(clippy::implicit_return)]
#![allow(missing_docs)]
// @generated automatically by Diesel CLI.

diesel::table! {
	media_archive (_id) {
		_id -> BigInt,
		media_id -> Text,
		provider -> Text,
		title -> Text,
		inserted_at -> Timestamp,
	}
}
