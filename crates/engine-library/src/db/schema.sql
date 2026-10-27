
	CREATE TABLE IF NOT EXISTS Information (
		id INTEGER PRIMARY KEY AUTOINCREMENT,
		uuid TEXT,
		schemaVersionMajor INTEGER,
		schemaVersionMinor INTEGER,
		schemaVersionPatch INTEGER,
		currentPlayedIndiciator INTEGER,
		lastRekordBoxLibraryImportReadCounter INTEGER
	);

	CREATE TABLE IF NOT EXISTS AlbumArt (
		id INTEGER PRIMARY KEY AUTOINCREMENT,
		hash TEXT,
		albumArt BLOB
	);

	CREATE TABLE IF NOT EXISTS Track (
		id INTEGER PRIMARY KEY AUTOINCREMENT,
		playOrder INTEGER,
		length INTEGER,
		bpm INTEGER,
		year INTEGER,
		path TEXT,
		filename TEXT,
		bitrate INTEGER,
		bpmAnalyzed REAL,
		albumArtId INTEGER,
		fileBytes INTEGER,
		title TEXT,
		artist TEXT,
		album TEXT,
		genre TEXT,
		comment TEXT,
		label TEXT,
		composer TEXT,
		remixer TEXT,
		key INTEGER,
		rating INTEGER,
		albumArt TEXT,
		timeLastPlayed DATETIME,
		isPlayed BOOLEAN,
		fileType TEXT,
		isAnalyzed BOOLEAN,
		dateCreated DATETIME,
		dateAdded DATETIME,
		isAvailable BOOLEAN,
		isMetadataOfPackedTrackChanged BOOLEAN,
		isPerfomanceDataOfPackedTrackChanged BOOLEAN,
		playedIndicator INTEGER,
		isMetadataImported BOOLEAN,
		pdbImportKey INTEGER,
		streamingSource TEXT,
		uri TEXT,
		isBeatGridLocked BOOLEAN,
		originDatabaseUuid TEXT,
		originTrackId INTEGER,
		streamingFlags INTEGER,
		explicitLyrics BOOLEAN,
		lastEditTime DATETIME,
		albumArtSourceHash CHAR(40),
		CONSTRAINT C_path UNIQUE (path)
	);

	CREATE TABLE IF NOT EXISTS PerformanceData (
		trackId INTEGER PRIMARY KEY,
		trackData BLOB,
		overviewWaveFormData BLOB,
		beatData BLOB,
		quickCues BLOB,
		loops BLOB,
		thirdPartySourceId INTEGER,
		activeOnLoadLoops INTEGER,
		FOREIGN KEY(trackId) REFERENCES Track(id) ON DELETE CASCADE ON UPDATE CASCADE
	);

	CREATE TABLE IF NOT EXISTS Playlist (
		id INTEGER PRIMARY KEY AUTOINCREMENT,
		title TEXT,
		parentListId INTEGER,
		isPersisted BOOLEAN,
		nextListId INTEGER,
		lastEditTime DATETIME,
		isExplicitlyExported BOOLEAN
	);

	CREATE TABLE IF NOT EXISTS PlaylistEntity (
		id INTEGER PRIMARY KEY AUTOINCREMENT,
		listId INTEGER,
		trackId INTEGER,
		databaseUuid TEXT,
		nextEntityId INTEGER,
		membershipReference INTEGER,
		FOREIGN KEY (listId) REFERENCES Playlist (id) ON DELETE CASCADE
	);

	CREATE TABLE IF NOT EXISTS Smartlist (
		listUuid TEXT NOT NULL PRIMARY KEY,
		title TEXT,
		parentPlaylistPath TEXT,
		nextPlaylistPath TEXT,
		nextListUuid TEXT,
		rules TEXT,
		lastEditTime DATETIME
	);
	