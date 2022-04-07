//! Module for SQL Utility functions

use diesel::prelude::*;
use std::{
	borrow::Cow,
	fs::File,
	io::BufReader,
	path::Path,
};

use super::archive::import::ImportProgress;

/// All migrations from "libytdlr/migrations" embedded into the binary
pub const MIGRATIONS: diesel_migrations::EmbeddedMigrations = diesel_migrations::embed_migrations!();

/// Open a SQLite Connection for `sqlite_path`
pub fn sqlite_connect<P: AsRef<Path>>(sqlite_path: P) -> Result<SqliteConnection, crate::Error> {
	return match sqlite_path.as_ref().to_str() {
		Some(path) => {
			let mut connection = SqliteConnection::establish(path)?;

			apply_sqlite_migrations(&mut connection)?;

			return Ok(connection);
		},
		None => Err(crate::Error::Other(format!("SQLite only accepts UTF-8 Paths, and given path failed to be converted to a string without being lossy, Path (converted lossy): \"{}\"", sqlite_path.as_ref().to_string_lossy()))),
	};
}

/// Apply all (up) migrations to a SQLite Database
#[inline]
fn apply_sqlite_migrations(connection: &mut SqliteConnection) -> Result<(), crate::Error> {
	let applied = diesel_migrations::MigrationHarness::run_pending_migrations(connection, MIGRATIONS)
		.map_err(|err| return crate::Error::Other(format!("Applying SQL Migrations Errored! Error:\n{}", err)))?;

	debug!("Applied Migrations: {:?}", applied);

	return Ok(());
}

/// Check if the input path is a sql database, if not migrate to sql and return new path and open connection
/// Parameter `pgcb` will be used when migration will be applied
///
/// This function is intendet to be used over [`sqlite_connect`] in all non-test cases
pub fn migrate_and_connect<S: FnMut(ImportProgress)>(
	archive_path: &Path,
	pgcb: S,
) -> Result<(Cow<Path>, SqliteConnection), crate::Error> {
	// let archive_path = archive_path.as_ref();
	let mut input_archive_reader = BufReader::new(File::open(archive_path)?);

	return Ok(
		match crate::main::archive::import::detect_archive_type(&mut input_archive_reader)? {
			super::archive::import::ArchiveType::Unknown => {
				return Err(crate::Error::Other(
					"Unknown Archive type to migrate, maybe try importing".into(),
				))
			},
			super::archive::import::ArchiveType::JSON => {
				debug!("Applying Migration from JSON to SQLite");
				let sqlite_path = {
					let mut tmp = archive_path.to_path_buf();
					tmp.set_extension("db");

					tmp
				};

				// handle case where the input path matches the changed path
				if sqlite_path == archive_path {
					return Err(crate::Error::Other(
						"Migration cannot be done: Input path matches output path (setting extension to \".db\")"
							.into(),
					));
				}

				let mut connection = sqlite_connect(&sqlite_path)?;

				crate::main::archive::import::import_ytdlr_json_archive(
					&mut input_archive_reader,
					&mut connection,
					pgcb,
				)?;

				debug!("Migration from JSON to SQLite done");

				(sqlite_path.into(), connection)
			},
			super::archive::import::ArchiveType::SQLite => (archive_path.into(), sqlite_connect(archive_path)?),
		},
	);
}

#[cfg(test)]
mod test {
	use super::*;
	use serial_test::serial;

	fn create_connection() -> SqliteConnection {
		// chrono is used to create a different database for each thread
		let path = std::env::temp_dir().join(format!("ytdl-test-sqlite/{}-sqlite.db", chrono::Utc::now()));

		// remove if already exists to have a clean test
		if path.exists() {
			std::fs::remove_file(&path).expect("Expected the file to be removed");
		}

		std::fs::create_dir_all(path.parent().expect("Expected the file to have a parent"))
			.expect("expected the directory to be created");

		return crate::main::sql_utils::sqlite_connect(path).expect("Expected SQLite to successfully start");
	}

	mod connect {
		use super::*;
		use std::{
			ffi::OsString,
			os::unix::prelude::OsStringExt,
		};

		#[test]
		fn test_connect() {
			let path = std::env::temp_dir().join(format!("ytdl-test-sqlite/{}-sqlite.db", chrono::Utc::now()));
			std::fs::create_dir_all(path.parent().expect("Expected the file to have a parent"))
				.expect("expected the directory to be created");

			let connection = sqlite_connect(path);

			assert!(connection.is_ok());
		}

		// it seems like non-utf8 paths are a pain to create os-independently, so it is just linux where the following works
		#[cfg(target_os = "linux")]
		#[test]
		fn test_connect_notutf8() {
			let path = OsString::from_vec(vec![255]);

			let err = sqlite_connect(path);

			assert!(err.is_err());
			// Not using "unwrap_err", because of https://github.com/diesel-rs/diesel/discussions/3124
			let err = match err {
				Ok(_) => panic!("Expected a Error value"),
				Err(err) => err,
			};
			// the following is only a "contains", because of the abitrary path that could be after it
			assert!(err.to_string().contains("SQLite only accepts UTF-8 Paths, and given path failed to be converted to a string without being lossy, Path (converted lossy):"));
		}
	}

	mod apply_sqlite_migrations {
		use super::*;

		#[test]
		fn test_all_migrations_applied() {
			let mut connection = create_connection();

			let res = diesel_migrations::MigrationHarness::has_pending_migration(&mut connection, MIGRATIONS);

			assert!(res.is_ok());
			let res = res.unwrap();
			assert_eq!(false, res); // explicit bool test
		}
	}

	mod migrate_and_connect {
		use std::{
			ffi::OsStr,
			io::{
				BufWriter,
				Write,
			},
			ops::Deref,
			path::PathBuf,
			sync::RwLock,
		};

		use super::*;

		fn gen_archive_path<P: AsRef<OsStr>>(extension: P) -> PathBuf {
			let mut path = std::env::temp_dir().join(format!("ytdl-test-sql_utils/{}-gen_archive", chrono::Utc::now()));
			path.set_extension(extension);

			return path;
		}

		fn clear_path<P: AsRef<Path>>(path: P) {
			let path = path.as_ref();

			if path.exists() {
				std::fs::remove_file(path).expect("Expected file to be removed");
			}
		}

		fn create_dir_all_parent<P: AsRef<Path>>(path: P) {
			let path = path.as_ref();
			std::fs::create_dir_all(path.parent().expect("Expected the file to have a parent"))
				.expect("expected the directory to be created");
		}

		fn write_file_with_content<S: AsRef<str>, P: AsRef<OsStr>>(input: S, extension: P) -> PathBuf {
			let path = gen_archive_path(extension);

			clear_path(&path);

			create_dir_all_parent(&path);

			let mut file = BufWriter::new(std::fs::File::create(&path).expect("Expected file to be created"));

			file.write_all(input.as_ref().as_bytes())
				.expect("Expected successfull file write");

			return path;
		}

		/// Test utility function for easy callbacks
		fn callback_counter(c: &RwLock<Vec<ImportProgress>>) -> impl FnMut(ImportProgress) + '_ {
			return |imp| c.write().expect("write failed").push(imp);
		}

		#[test]
		fn test_unknown_archive() {
			let string0 = "
			youtube ____________
			youtube ------------
			youtube aaaaaaaaaaaa
			soundcloud 0000000000
			";

			let path = write_file_with_content(string0, "unknown_ytdl");

			let pgcounter = RwLock::new(Vec::<ImportProgress>::new());

			let res = migrate_and_connect(&path, callback_counter(&pgcounter));

			assert!(res.is_err());
			let res = match res {
				Ok(_) => panic!("Expected a Error value"),
				Err(err) => err,
			};

			assert!(res
				.to_string()
				.contains("Unknown Archive type to migrate, maybe try importing"));
			assert_eq!(0, pgcounter.read().expect("read failed").len());
		}

		#[test]
		fn test_sqlite_archive() {
			let path = gen_archive_path("db_sqlite");
			create_dir_all_parent(&path);

			{
				// create database file
				let _connection = sqlite_connect(&path);
				// and drop it, so to not have a lock on it
			}

			let pgcounter = RwLock::new(Vec::<ImportProgress>::new());

			let res = migrate_and_connect(&path, callback_counter(&pgcounter));

			assert!(res.is_ok());
			let res = res.unwrap();

			assert_eq!(&path, res.0.as_ref());
			assert_eq!(0, pgcounter.read().expect("read failed").len());
		}

		#[test]
		#[serial]
		fn test_json_archive() {
			let string0 = r#"
			{
				"version": "0.1.0",
				"videos": [
					{
						"id": "____________",
						"provider": "youtube",
						"dlFinished": true,
						"editAsked": true,
						"fileName": "someFile1.mp3"
					},
					{
						"id": "------------",
						"provider": "youtube",
						"dlFinished": false,
						"editAsked": true,
						"fileName": "someFile2.mp3"
					},
					{
						"id": "aaaaaaaaaaaa",
						"provider": "youtube",
						"dlFinished": true,
						"editAsked": false,
						"fileName": "someFile3.mp3"
					},
					{
						"id": "0000000000",
						"provider": "soundcloud",
						"dlFinished": true,
						"editAsked": true,
						"fileName": "someFile4.mp3"
					}
				]
			}
			"#;

			let path = write_file_with_content(string0, "json_json");

			let expected_path = {
				let mut tmp = path.to_path_buf();
				tmp.set_extension("db");

				tmp
			};

			clear_path(&expected_path);

			let pgcounter = RwLock::new(Vec::<ImportProgress>::new());

			let res = migrate_and_connect(&path, callback_counter(&pgcounter));

			assert!(res.is_ok());
			let res = res.unwrap();

			assert_eq!(&expected_path, res.0.as_ref());
			assert_eq!(
				&vec![
					ImportProgress::Starting,
					ImportProgress::SizeHint(4), // Size Hint of 4, because of a intermediate array length
					// index start at 0, thanks to json array index
					ImportProgress::Increase(1, 0),
					ImportProgress::Increase(1, 1),
					ImportProgress::Increase(1, 2),
					ImportProgress::Increase(1, 3),
					ImportProgress::Finished(4)
				],
				pgcounter.read().expect("failed to read").deref()
			);
		}

		#[test]
		#[serial]
		fn test_json_archive_same_name() {
			let string0 = r#"
			{
			}
			"#;

			let path = write_file_with_content(string0, "db");

			let pgcounter = RwLock::new(Vec::<ImportProgress>::new());

			let res = migrate_and_connect(&path, callback_counter(&pgcounter));

			assert!(res.is_err());
			let res = match res {
				Ok(_) => panic!("Expected a Error value"),
				Err(err) => err,
			};

			assert_eq!(
				res.to_string(),
				"Other: Migration cannot be done: Input path matches output path (setting extension to \".db\")"
			);
			assert_eq!(0, pgcounter.read().expect("read failed").len());
		}
	}
}
