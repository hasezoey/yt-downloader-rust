//! Module for SQL Utility functions

use diesel::prelude::*;
use std::{
	borrow::Cow,
	fs::File,
	io::BufReader,
	path::Path,
};

use crate::error::IOErrorToError;

use super::archive::import::{
	ArchiveType,
	ImportProgress,
	detect_archive_type,
};

/// All migrations from "libytdlr/migrations" embedded into the binary
pub const MIGRATIONS: diesel_migrations::EmbeddedMigrations = diesel_migrations::embed_migrations!();

/// Open a SQLite Connection for `sqlite_path` and apply sqlite migrations.
///
/// Does not migrate archive formats, use [migrate_and_connect] instead.
pub fn sqlite_connect<P: AsRef<Path>>(sqlite_path: P) -> Result<SqliteConnection, crate::Error> {
	// having to convert the path to "str" because diesel (and underlying sqlite library) only accept strings
	return match sqlite_path.as_ref().to_str() {
		Some(path) => {
			let mut connection = SqliteConnection::establish(path)?;

			apply_sqlite_migrations(&mut connection)?;

			return Ok(connection);
		},
		None => Err(crate::Error::other(format!(
			"SQLite only accepts UTF-8 Paths, and given path failed to be converted to a string without being lossy, Path (converted lossy): \"{}\"",
			sqlite_path.as_ref().to_string_lossy()
		))),
	};
}

/// Apply all (up) migrations to a SQLite Database
#[inline]
fn apply_sqlite_migrations(connection: &mut SqliteConnection) -> Result<(), crate::Error> {
	let applied = diesel_migrations::MigrationHarness::run_pending_migrations(connection, MIGRATIONS)
		.map_err(|err| return crate::Error::other(format!("Applying SQL Migrations Errored! Error:\n{err}")))?;

	debug!("Applied Migrations: {:?}", applied);

	return Ok(());
}

/// Check if the input path is a sql database, if not migrate to sql and return new path and open connection.
///
/// Parameter `pgcb` will be used when migration will be applied.
///
/// This function is intended to be used over [`sqlite_connect`] in all non-test cases.
pub fn migrate_and_connect<S: FnMut(ImportProgress)>(
	archive_path: &Path,
	_pgcb: S,
) -> Result<(Cow<'_, Path>, SqliteConnection), crate::Error> {
	// early return in case the file does not actually exist
	if !archive_path.exists() {
		return Ok((archive_path.into(), sqlite_connect(archive_path)?));
	}

	let migrate_to_path = {
		let mut tmp = archive_path.to_path_buf();
		tmp.set_extension("db");

		tmp
	};

	// check if the "migrate-to" path already exists, and use that directly instead or error of already existing
	if migrate_to_path.exists() {
		if !migrate_to_path.is_file() {
			return Err(crate::Error::not_a_file(
				"Migrate-To Path exists but is not a file!",
				migrate_to_path,
			));
		}

		let mut sqlite_path_reader = BufReader::new(File::open(&migrate_to_path).attach_path_err(&migrate_to_path)?);
		return Ok(match detect_archive_type(&mut sqlite_path_reader)? {
			ArchiveType::Unknown => {
				return Err(crate::Error::other(format!(
					"Migrate-To Path already exists, but is of unknown type! Path: \"{}\"",
					migrate_to_path.to_string_lossy()
				)));
			},
			ArchiveType::JSON => {
				return Err(crate::Error::other(format!(
					"Migrate-To Path already exists and is a JSON archive, please rename it and retry the migration! Path: \"{}\"",
					migrate_to_path.to_string_lossy()
				)));
			},
			ArchiveType::SQLite => {
				// this has to be done before, because the following ".into" call will move the value
				let connection = sqlite_connect(&migrate_to_path)?;

				(migrate_to_path.into(), connection)
			},
		});
	}

	let mut input_archive_reader = BufReader::new(File::open(archive_path).attach_path_err(archive_path)?);

	return Ok(match detect_archive_type(&mut input_archive_reader)? {
		ArchiveType::Unknown => {
			return Err(crate::Error::other(
				"Unknown Archive type to migrate, maybe try importing",
			));
		},
		ArchiveType::JSON => {
			return Err(crate::Error::other(
				"JSON archive is now unsupported, please use version <=0.10.0 to import it.",
			));
		},
		ArchiveType::SQLite => (archive_path.into(), sqlite_connect(archive_path)?),
	});
}

#[cfg(test)]
mod test {
	use super::*;
	use tempfile::{
		Builder as TempBuilder,
		TempDir,
	};

	fn create_connection() -> (SqliteConnection, TempDir) {
		let testdir = TempBuilder::new()
			.prefix("ytdl-test-sqlite-")
			.tempdir()
			.expect("Expected a temp dir to be created");
		// chrono is used to create a different database for each thread
		let path = testdir.as_ref().join(format!("{}-sqlite.db", chrono::Utc::now()));

		// remove if already exists to have a clean test
		if path.exists() {
			std::fs::remove_file(&path).expect("Expected the file to be removed");
		}

		return (
			crate::main::sql_utils::sqlite_connect(&path).expect("Expected SQLite to successfully start"),
			testdir,
		);
	}

	mod connect {
		use super::*;
		use std::{
			ffi::OsString,
			os::unix::prelude::OsStringExt,
		};

		#[test]
		fn test_connect() {
			let testdir = TempBuilder::new()
				.prefix("ytdl-test-sqliteConnect-")
				.tempdir()
				.expect("Expected a temp dir to be created");
			let path = testdir.as_ref().join(format!("{}-sqlite.db", chrono::Utc::now()));
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
			let (mut connection, _tempdir) = create_connection();

			let res = diesel_migrations::MigrationHarness::has_pending_migration(&mut connection, MIGRATIONS);

			assert!(res.is_ok());
			let res = res.unwrap();
			assert!(!res);
		}
	}

	mod migrate_and_connect {
		use std::{
			ffi::OsStr,
			io::{
				BufWriter,
				Write,
			},
			path::PathBuf,
			sync::RwLock,
		};

		use super::*;

		fn gen_archive_path<P: AsRef<OsStr>>(extension: P) -> (PathBuf, TempDir) {
			let testdir = TempBuilder::new()
				.prefix("ytdl-test-sqliteMigrate-")
				.tempdir()
				.expect("Expected a temp dir to be created");
			let mut path = testdir.as_ref().join(format!("{}-gen_archive", uuid::Uuid::new_v4()));
			path.set_extension(extension);
			println!("generated: {}", path.to_string_lossy());

			// clear generated path
			clear_path(&path);

			{
				let mut migrate_to_path = path.clone();
				migrate_to_path.set_extension("db");

				// clear migrate_to_path
				clear_path(migrate_to_path);
			}

			return (path, testdir);
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

		fn write_file_with_content<S: AsRef<str>, P: AsRef<OsStr>>(input: S, extension: P) -> (PathBuf, TempDir) {
			let (path, tempdir) = gen_archive_path(extension);

			create_dir_all_parent(&path);

			let mut file = BufWriter::new(std::fs::File::create(&path).expect("Expected file to be created"));

			file.write_all(input.as_ref().as_bytes())
				.expect("Expected successfull file write");

			return (path, tempdir);
		}

		/// Test utility function for easy callbacks
		fn callback_counter(c: &RwLock<Vec<ImportProgress>>) -> impl FnMut(ImportProgress) + '_ {
			return |imp| c.write().expect("write failed").push(imp);
		}

		#[test]
		fn test_input_unknown_archive() {
			let string0 = "
			youtube ____________
			youtube ------------
			youtube aaaaaaaaaaaa
			soundcloud 0000000000
			";

			let (path, _tempdir) = write_file_with_content(string0, "unknown_ytdl");

			let pgcounter = RwLock::new(Vec::<ImportProgress>::new());

			let res = migrate_and_connect(&path, callback_counter(&pgcounter));

			assert!(res.is_err());
			let res = match res {
				Ok(_) => panic!("Expected a Error value"),
				Err(err) => err,
			};

			assert!(
				res.to_string()
					.contains("Unknown Archive type to migrate, maybe try importing")
			);
			assert_eq!(0, pgcounter.read().expect("read failed").len());
		}

		#[test]
		fn test_input_sqlite_archive() {
			let (path, _tempdir) = gen_archive_path("db_sqlite");
			create_dir_all_parent(&path);

			{
				// create database file
				assert!(sqlite_connect(&path).is_ok());
			}

			let pgcounter = RwLock::new(Vec::<ImportProgress>::new());

			let res = migrate_and_connect(&path, callback_counter(&pgcounter));

			assert!(res.is_ok());
			let res = res.unwrap();

			assert_eq!(&path, res.0.as_ref());
			assert_eq!(0, pgcounter.read().expect("read failed").len());
		}

		#[test]
		fn test_input_json_archive_err() {
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
					}
				]
			}
			"#;

			let (path, _tempdir) = write_file_with_content(string0, "json_json");

			let expected_path = {
				let mut tmp = path.clone();
				tmp.set_extension("db");

				tmp
			};

			clear_path(&expected_path);

			let pgcounter = RwLock::new(Vec::<ImportProgress>::new());

			// we cannot use "unwrap_err" as "SqliteConnection" does not impl "Debug"
			let Err(res) = migrate_and_connect(&path, callback_counter(&pgcounter)) else {
				panic!("Expected Err variant");
			};

			assert!(res.to_string().contains("JSON archive is now unsupported"));
		}

		#[test]
		fn test_to_existing_json() {
			let string0 = r#"
			{
			}
			"#;

			let (path, _tempdir) = write_file_with_content(string0, "db");

			let pgcounter = RwLock::new(Vec::<ImportProgress>::new());

			let res = migrate_and_connect(&path, callback_counter(&pgcounter));

			assert!(res.is_err());
			let res = match res {
				Ok(_) => panic!("Expected a Error value"),
				Err(err) => err,
			};

			assert_eq!(
				res.to_string(),
				format!(
					"Other: Migrate-To Path already exists and is a JSON archive, please rename it and retry the migration! Path: \"{}\"",
					path.to_string_lossy()
				)
			);
			assert_eq!(0, pgcounter.read().expect("read failed").len());
		}

		#[test]
		fn test_to_existing_unknown() {
			let string0 = "
			youtube ____________
			youtube ------------
			youtube aaaaaaaaaaaa
			soundcloud 0000000000
			";

			let (path, _tempdir) = write_file_with_content(string0, "db");

			let pgcounter = RwLock::new(Vec::<ImportProgress>::new());

			let res = migrate_and_connect(&path, callback_counter(&pgcounter));

			assert!(res.is_err());
			let res = match res {
				Ok(_) => panic!("Expected a Error value"),
				Err(err) => err,
			};

			assert_eq!(
				res.to_string(),
				format!(
					"Other: Migrate-To Path already exists, but is of unknown type! Path: \"{}\"",
					path.to_string_lossy()
				)
			);
			assert_eq!(0, pgcounter.read().expect("read failed").len());
		}

		#[test]
		fn test_to_existing_sqlite() {
			let (path, _tempdir) = gen_archive_path("db");
			create_dir_all_parent(&path);

			{
				// create database file
				assert!(sqlite_connect(&path).is_ok());
			}

			let pgcounter = RwLock::new(Vec::<ImportProgress>::new());

			let res = migrate_and_connect(&path, callback_counter(&pgcounter));

			assert!(res.is_ok());
			let res = res.unwrap();

			assert_eq!(&path, res.0.as_ref());
			assert_eq!(0, pgcounter.read().expect("read failed").len());
		}
	}
}
