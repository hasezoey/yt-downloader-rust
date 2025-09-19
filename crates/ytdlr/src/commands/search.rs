use indicatif::ProgressBar;

use crate::{
	clap_conf::{
		ArchiveSearch,
		CliDerive,
		SearchResultFormat,
	},
	utils,
};
use diesel::prelude::*;
use libytdlr::{
	chrono::Utc,
	data::{
		sql_models::Media,
		sql_schema::media_archive,
	},
	diesel,
};

/// Helper function to convert a given input to a "LIKE" query (appending "%")
fn to_like_query(input: &str) -> String {
	let mut res: String = input.to_owned();
	res.push('%');
	return res;
}

/// Handler function for the "archive search" subcommand
/// This function is mainly to keep the code structured and sorted
#[inline]
pub fn command_search(main_args: &CliDerive, sub_args: &ArchiveSearch) -> Result<(), crate::Error> {
	let Some(archive_path) = main_args.archive_path.as_ref() else {
		return Err(crate::Error::other("Archive is required for Search!"));
	};

	let bar: ProgressBar = ProgressBar::hidden();
	// dont set progress bar target, only required for handle_connect currently
	// crate::utils::set_progressbar(&bar, main_args);

	let (_new_archive, mut connection) = utils::handle_connect(archive_path, &bar, main_args)?;

	let mut query = media_archive::dsl::media_archive
		.into_boxed()
		.order(media_archive::_id.asc())
		.limit(sub_args.limit);

	for q in &sub_args.queries {
		match q.0 {
			crate::clap_conf::ArchiveSearchColumn::Provider => {
				query = query.or_filter(media_archive::columns::provider.like(to_like_query(&q.1)));
			},
			crate::clap_conf::ArchiveSearchColumn::MediaId => {
				query = query.or_filter(media_archive::columns::media_id.like(to_like_query(&q.1)));
			},
			crate::clap_conf::ArchiveSearchColumn::Title => {
				query = query.or_filter(media_archive::columns::title.like(to_like_query(&q.1)));
			},
			crate::clap_conf::ArchiveSearchColumn::InsertedAt => {
				let search_query = &q.1;
				if let Some(search_query) = search_query.strip_prefix(">=") {
					query = query.or_filter(media_archive::columns::inserted_at.ge(search_query));
				} else if let Some(search_query) = search_query.strip_prefix("<=") {
					query = query.or_filter(media_archive::columns::inserted_at.le(search_query));
				} else if let Some(search_query) = search_query.strip_prefix('<') {
					query = query.or_filter(media_archive::columns::inserted_at.lt(search_query));
				} else if let Some(search_query) = search_query.strip_prefix('>') {
					query = query.or_filter(media_archive::columns::inserted_at.gt(search_query));
				} else if let Some(search_query) = search_query.strip_prefix('=') {
					query = query.or_filter(media_archive::columns::inserted_at.eq(search_query));
				} else {
					query = query.or_filter(media_archive::columns::inserted_at.eq(&search_query[..]));
				}
			},
		}
	}

	let lines_iter = query.load::<Media>(&mut connection)?;

	if lines_iter.is_empty() {
		println!("No Results found");
		return Ok(());
	}

	// print header, if header is required
	match sub_args.result_format {
		SearchResultFormat::Normal => (),
		SearchResultFormat::CSVC => {
			println!("provider,media_id,inserted_at,title");
		},
		SearchResultFormat::CSVT => {
			println!("provider\tmedia_id\tinserted_at\ttitle");
		},
	}

	for media in lines_iter {
		// required, otherwise formatting as "%+" / "RFC3339" is not possible for NaiveDateTime
		let inserted_at = media
			.inserted_at
			.and_local_timezone(Utc)
			.single()
			.expect("Expected to properly convert with timezone")
			.format("%+");
		match sub_args.result_format {
			SearchResultFormat::Normal => {
				println!(
					"[{}:{}] [{}] {}",
					media.provider, media.media_id, inserted_at, media.title
				);
			},
			SearchResultFormat::CSVC => {
				println!(
					"{},{},\"{}\",\"{}\"",
					media.provider, media.media_id, inserted_at, media.title
				);
			},
			SearchResultFormat::CSVT => {
				println!(
					"{}\t{}\t\"{}\"\t\"{}\"",
					media.provider, media.media_id, inserted_at, media.title
				);
			},
		}
	}

	return Ok(());
}
