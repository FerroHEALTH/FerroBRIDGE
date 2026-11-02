// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The durable watermark of each reader of the AQL source, in a `redb` file.
//!
//! One row per reader name holds the commit instant the reader has finished
//! up to, as the CDR wrote it, and the version uids finished at that
//! instant. A write is one ACID transaction
//! (<https://docs.rs/redb/4.3.0/redb/>) that refuses to move a watermark
//! backwards, so a crash leaves either the old watermark or the new one. The
//! file holds instants and version uids and nothing else. No specification
//! governs this: our own design.

use std::collections::BTreeSet;
use std::path::Path;
use std::path::PathBuf;

use redb::Database;
use redb::ReadableDatabase;
use redb::ReadableTable;
use redb::TableDefinition;
use serde::Deserialize;
use serde::Serialize;

use crate::change_feed::Committed;
use crate::change_feed::FeedError;
use crate::change_feed::Position;
use crate::change_feed::ReaderName;

/// Reader name to its watermark, as JSON.
const WATERMARKS: TableDefinition<'static, &str, &str> = TableDefinition::new("reader_watermark");

/// The watermarks of every reader, on disk.
#[derive(Debug)]
pub struct WatermarkStore {
    /// The open database.
    database: Database,
    /// The path it was opened at.
    path: PathBuf,
}

/// One watermark as it is stored.
#[derive(Debug, Serialize, Deserialize)]
struct Stored {
    /// The instant as the CDR wrote it.
    at: String,
    /// The version uids finished at the instant.
    versions: BTreeSet<String>,
}

/// Returns the store error `source` is.
fn store(source: impl Into<redb::Error>) -> FeedError {
    FeedError::Store {
        source: Box::new(source.into()),
    }
}

impl WatermarkStore {
    /// Opens, or creates, the store at `path`.
    ///
    /// # Errors
    /// Returns [`FeedError::Store`] when the file cannot be opened or created
    /// or its table cannot be created.
    pub fn open(path: &Path) -> Result<Self, FeedError> {
        let database = Database::create(path).map_err(store)?;
        let write = database.begin_write().map_err(store)?;
        {
            let _watermarks = write.open_table(WATERMARKS).map_err(store)?;
        }
        write.commit().map_err(store)?;
        Ok(Self {
            database,
            path: path.to_path_buf(),
        })
    }

    /// Returns the path the store was opened at.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Returns the watermark of `reader`, or `None` before its first one.
    ///
    /// # Errors
    /// Returns [`FeedError::Store`] when the read fails and
    /// [`FeedError::Watermark`] when the stored row does not read.
    pub fn get(&self, reader: &ReaderName) -> Result<Option<Committed>, FeedError> {
        let read = self.database.begin_read().map_err(store)?;
        let table = read.open_table(WATERMARKS).map_err(store)?;
        let Some(row) = table.get(reader.as_str()).map_err(store)? else {
            return Ok(None);
        };
        decode(reader, row.value()).map(Some)
    }

    /// Moves the watermark of `reader` to `watermark`.
    ///
    /// # Errors
    /// Returns [`FeedError::Behind`] when `watermark` does not lie after the
    /// stored one, [`FeedError::Store`] when the write does not commit, and
    /// [`FeedError::Watermark`] when the stored row does not read.
    pub fn set(&self, reader: &ReaderName, watermark: &Committed) -> Result<(), FeedError> {
        let write = self.database.begin_write().map_err(store)?;
        {
            let mut table = write.open_table(WATERMARKS).map_err(store)?;
            let stored = match table.get(reader.as_str()).map_err(store)? {
                Some(row) => Some(decode(reader, row.value())?),
                None => None,
            };
            if let Some(stored) = stored
                && stored != *watermark
                && !watermark.follows(&stored)
            {
                return Err(FeedError::Behind {
                    reader: reader.clone(),
                    position: Position::Committed(watermark.clone()),
                });
            }
            let row = serde_json::to_string(&Stored {
                at: watermark.text().to_owned(),
                versions: watermark.versions().clone(),
            })
            .map_err(|source| FeedError::Watermark {
                reader: reader.clone(),
                source,
            })?;
            table.insert(reader.as_str(), row.as_str()).map_err(store)?;
        }
        write.commit().map_err(store)
    }
}

/// Returns the watermark `row` stores for `reader`.
fn decode(reader: &ReaderName, row: &str) -> Result<Committed, FeedError> {
    let stored: Stored = serde_json::from_str(row).map_err(|source| FeedError::Watermark {
        reader: reader.clone(),
        source,
    })?;
    let at = stored
        .at
        .parse::<jiff::Timestamp>()
        .map_err(|_refused| FeedError::Malformed {
            reason: format!("the stored watermark of reader {reader} names no instant"),
            source: None,
        })?;
    Ok(Committed::new(at, stored.at, stored.versions))
}
