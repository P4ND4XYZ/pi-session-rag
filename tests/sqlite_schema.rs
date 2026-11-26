use rusqlite::{Connection, ffi, params};

#[test]
fn rejects_duplicate_pi_session_ids_even_from_different_paths() {
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch(include_str!("../src/schema.sql"))
        .unwrap();

    let insert = "INSERT INTO sessions (pi_session_id, source_path) VALUES (?1, ?2)";
    conn.execute(insert, params!["pi-1", "first.jsonl"])
        .unwrap();
    conn.execute(insert, params!["pi-2", "second.jsonl"])
        .unwrap();

    let err = conn
        .execute(insert, params!["pi-1", "copy.jsonl"])
        .unwrap_err();
    assert_eq!(
        err.sqlite_error().map(|err| err.extended_code),
        Some(ffi::SQLITE_CONSTRAINT_UNIQUE),
    );
}
