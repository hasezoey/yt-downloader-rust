//! Module for SQL Diesel Models

use crate::data::sql_schema::media_archive;
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
pub struct InsMedia<'a> {
	/// The ID of the media given used by the provider
	pub media_id: &'a str,
	/// The Provider from where this media was downloaded from
	pub provider: &'a str,
	/// The Title the media has
	pub title:    &'a str,
}

impl<'a> InsMedia<'a> {
	/// Create a new instance of [InsMedia]
	pub fn new(media_id: &'a str, provider: &'a str, title: &'a str) -> Self {
		return Self {
			media_id,
			provider,
			title,
		};
	}
}
