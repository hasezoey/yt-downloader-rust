//! Module for SQL Diesel Models

use crate::data::sql_schema::*;
use chrono::NaiveDateTime;
use diesel::prelude::*;

/// Struct representing a Media table entry
#[derive(Debug, Clone, PartialEq, Queryable)]
#[diesel(table_name = media_archive)]
pub struct Media {
	/// The ID of the video, auto-incremented upwards
	pub _id:         i64,
	/// The ID of the media given used by the provider
	pub media_id:    String,
	/// The Provider from where this media was downloaded from
	pub provider:    String,
	/// The Title the media has
	pub title:       String,
	/// The Time this media was inserted into the database
	pub inserted_at: NaiveDateTime,
}

/// Struct for inserting a [Media] into the database
#[derive(Debug, Clone, PartialEq, Insertable)]
#[diesel(table_name = media_archive)]
pub struct InsMedia {
	/// The ID of the media given used by the provider
	pub media_id: String,
	/// The Provider from where this media was downloaded from
	pub provider: String,
	/// The Title the media has
	pub title:    String,
}

impl InsMedia {
	/// Create a new instance of [InsMedia]
	pub fn new<MID: AsRef<str>, P: AsRef<str>, T: AsRef<str>>(media_id: MID, provider: P, title: T) -> Self {
		return Self {
			media_id: media_id.as_ref().into(),
			provider: provider.as_ref().into(),
			title:    title.as_ref().into(),
		};
	}
}
