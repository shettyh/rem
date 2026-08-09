use std::sync::{Arc, Barrier};
use std::thread;

use rem_core::{default_database_path, Collection, CollectionError, CURRENT_SCHEMA_VERSION};
use rusqlite::Connection;
use tempfile::tempdir;

#[test]
fn a_new_collection_initializes_and_reopens_at_revision_zero() {
    let temp = tempdir().unwrap();
    let path = temp.path().join("nested").join("collection.sqlite3");

    let collection = Collection::open(&path).unwrap();
    assert_eq!(collection.sync_revision().unwrap(), 0);
    drop(collection);

    let reopened = Collection::open(&path).unwrap();
    assert_eq!(reopened.sync_revision().unwrap(), 0);
}

#[test]
fn independent_handles_can_open_the_same_collection() {
    let temp = tempdir().unwrap();
    let path = temp.path().join("collection.sqlite3");

    let first = Collection::open(&path).unwrap();
    let second = Collection::open(&path).unwrap();

    assert_eq!(first.sync_revision().unwrap(), 0);
    assert_eq!(second.sync_revision().unwrap(), 0);
}

#[test]
fn concurrent_first_open_initializes_the_collection_once() {
    let temp = tempdir().unwrap();
    let path = temp.path().join("first-open.sqlite3");
    let barrier = Arc::new(Barrier::new(2));
    let handles: Vec<_> = (0..2)
        .map(|_| {
            let path = path.clone();
            let barrier = barrier.clone();
            thread::spawn(move || {
                barrier.wait();
                Collection::open(path).map(|collection| collection.sync_revision().unwrap())
            })
        })
        .collect();

    for handle in handles {
        assert_eq!(handle.join().unwrap().unwrap(), 0);
    }
}

#[test]
fn opening_a_newer_schema_is_rejected_without_modifying_it() {
    let temp = tempdir().unwrap();
    let path = temp.path().join("future.sqlite3");
    let connection = Connection::open(&path).unwrap();
    connection
        .pragma_update(None, "user_version", CURRENT_SCHEMA_VERSION + 1)
        .unwrap();
    drop(connection);

    let error = Collection::open(&path).unwrap_err();
    assert!(matches!(
        error,
        CollectionError::NewerSchema { found, supported }
            if found == CURRENT_SCHEMA_VERSION + 1 && supported == CURRENT_SCHEMA_VERSION
    ));

    let connection = Connection::open(&path).unwrap();
    let version: u32 = connection
        .pragma_query_value(None, "user_version", |row| row.get(0))
        .unwrap();
    assert_eq!(version, CURRENT_SCHEMA_VERSION + 1);
}

#[test]
fn the_default_database_path_has_a_stable_filename() {
    let path = default_database_path().unwrap();
    assert_eq!(path.file_name().unwrap(), "collection.sqlite3");
}
