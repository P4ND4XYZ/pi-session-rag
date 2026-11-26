PRAGMA foreign_keys = ON;

CREATE TABLE IF NOT EXISTS sessions (
    id INTEGER PRIMARY KEY,
    pi_session_id TEXT NOT NULL UNIQUE,
    source_path TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS messages (
    session_id INTEGER NOT NULL REFERENCES sessions(id),
    pi_entry_id TEXT NOT NULL,
    text TEXT NOT NULL
);
