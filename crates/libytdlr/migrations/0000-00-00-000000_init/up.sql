CREATE TABLE media_archive (
	_id INTEGER NOT NULL PRIMARY KEY,
	media_id VARCHAR NOT NULL,
	provider VARCHAR NOT NULL,
	title VARCHAR NOT NULL,
	inserted_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE UNIQUE INDEX media_archive_unique ON media_archive (media_id, provider);
