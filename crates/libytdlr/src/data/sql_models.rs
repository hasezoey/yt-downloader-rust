//! Module for SQL Diesel Models

use crate::data::sql_schema::*;
use chrono::NaiveDateTime;
use diesel::prelude::*;

#[derive(Debug, Clone, PartialEq, Queryable)]
#[diesel(table_name = media_archive)]
// #[diesel(treat_none_as_default_value = false)]
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

#[derive(Debug, Clone, PartialEq, Insertable)]
#[diesel(table_name = media_archive)]
// #[diesel(treat_none_as_default_value = false)]
pub struct InsMedia {
	/// The ID of the media given used by the provider
	pub media_id: String,
	/// The Provider from where this media was downloaded from
	pub provider: String,
	/// The Title the media has
	pub title:    String,
}

impl InsMedia {
	pub fn new<MID: AsRef<str>, P: AsRef<str>, T: AsRef<str>>(media_id: MID, provider: P, title: T) -> Self {
		return Self {
			media_id: media_id.as_ref().into(),
			provider: provider.as_ref().into(),
			title:    title.as_ref().into(),
		};
	}
}
