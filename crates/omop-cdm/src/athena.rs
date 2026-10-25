// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The loader of an OHDSI Athena vocabulary download into the vocabulary
//! tables of a CDM schema.
//!
//! The CDM defines the ten vocabulary tables
//! (<https://ohdsi.github.io/CommonDataModel/cdm54.html#Vocabulary_Tables>)
//! and says nothing of the files they are distributed in. The shape read here
//! is the one the Athena application writes, read from its source at the
//! commit `docs/specs/athena/PROVENANCE.md` pins: one file per table named in
//! upper case (`CONCEPT.csv`), a header row of column names, a tab between
//! values, no quoting, `\n` after each row, UTF-8, an empty field for null,
//! and dates as `yyyyMMdd`.
//!
//! [`Export::read`] finds the files in a directory: the nine tables Athena
//! serves are required, `SOURCE_TO_CONCEPT_MAP.csv`, a site's own table, is
//! loaded when present, and every other file is listed and left alone.
//! [`load`] then replaces each table's rows in a transaction of its own
//! through `COPY`, with the column list taken from the header row and checked
//! against the CDM definitions, and reports the vocabulary version.
//!
//! No specification governs the loading: our own design.

use crate::database::CdmPool;
use crate::meta::{self, TableMeta};
use std::fmt;
use std::path::{Path, PathBuf};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, BufReader};

/// The tables an Athena download carries, in the order they are loaded.
const SERVED: [&str; 9] = [
    "vocabulary",
    "domain",
    "concept_class",
    "relationship",
    "concept",
    "concept_relationship",
    "concept_synonym",
    "concept_ancestor",
    "drug_strength",
];

/// The vocabulary table a site fills itself, loaded when its file is present.
const SITE: &str = "source_to_concept_map";

/// The byte `COPY` is told quotes a value.
///
/// The download quotes nothing, so the quote is a control character no
/// vocabulary text holds, and a file that carries it is refused before
/// `COPY` could read it as a quote.
const QUOTE: u8 = 0x08;

/// How much of a file is sent to `COPY` at a time.
const CHUNK: usize = 1 << 20;

/// One file of a download and the table it fills.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExportFile {
    table: &'static TableMeta,
    path: PathBuf,
}

impl ExportFile {
    /// Returns the table the file fills.
    #[must_use]
    pub fn table(&self) -> &'static TableMeta {
        self.table
    }

    /// Returns where the file is.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }
}

/// The files of a download, found in one directory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Export {
    files: Vec<ExportFile>,
    ignored: Vec<String>,
}

/// A directory that is not a download the loader reads.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum ExportError {
    /// The directory could not be listed.
    #[error("the vocabulary directory {directory} could not be read")]
    Directory {
        /// The directory.
        directory: PathBuf,
        /// What the file system reported.
        #[source]
        source: std::io::Error,
    },
    /// Files of the tables a download carries are missing.
    #[error(
        "the vocabulary directory {directory} lacks {list}, which every Athena download carries",
        list = missing.join(", ")
    )]
    Missing {
        /// The directory.
        directory: PathBuf,
        /// The file names it lacks.
        missing: Vec<String>,
    },
}

impl Export {
    /// Finds the files of the download in `directory`.
    ///
    /// A file is taken by its exact name, the table name in upper case with
    /// `.csv` appended; every other entry is listed in [`Export::ignored`].
    ///
    /// # Errors
    ///
    /// Returns [`ExportError::Directory`] when the directory cannot be read,
    /// and [`ExportError::Missing`] naming every file of the nine served
    /// tables that is absent.
    pub fn read(directory: &Path) -> Result<Self, ExportError> {
        let unreadable = |source| ExportError::Directory {
            directory: directory.to_path_buf(),
            source,
        };
        let mut names = Vec::new();
        for entry in std::fs::read_dir(directory).map_err(unreadable)? {
            let entry = entry.map_err(unreadable)?;
            names.push(entry.file_name().to_string_lossy().into_owned());
        }
        names.sort();
        let mut files = Vec::new();
        let mut missing = Vec::new();
        for name in SERVED.into_iter().chain([SITE]) {
            let file = file_name(name);
            if names.contains(&file) {
                if let Some(table) = meta::table(name) {
                    files.push(ExportFile {
                        table,
                        path: directory.join(&file),
                    });
                }
            } else if name != SITE {
                missing.push(file);
            }
        }
        if !missing.is_empty() {
            return Err(ExportError::Missing {
                directory: directory.to_path_buf(),
                missing,
            });
        }
        let taken: Vec<String> = files
            .iter()
            .map(|file| file_name(file.table.name))
            .collect();
        let ignored = names
            .into_iter()
            .filter(|name| !taken.contains(name))
            .collect();
        Ok(Self { files, ignored })
    }

    /// Returns the files that are loaded, in load order.
    #[must_use]
    pub fn files(&self) -> &[ExportFile] {
        &self.files
    }

    /// Returns the names of the directory's other entries, which are not
    /// loaded, in order.
    #[must_use]
    pub fn ignored(&self) -> &[String] {
        &self.ignored
    }
}

/// Returns the name of the file that fills `table`.
fn file_name(table: &str) -> String {
    format!("{}.csv", table.to_uppercase())
}

/// What [`load`] wrote.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Loaded {
    tables: Vec<(&'static str, u64)>,
    vocabulary_version: Option<String>,
}

impl Loaded {
    /// Returns each loaded table with the rows written into it, in load
    /// order.
    #[must_use]
    pub fn tables(&self) -> &[(&'static str, u64)] {
        &self.tables
    }

    /// Returns the release the download is, the `vocabulary_version` of the
    /// `VOCABULARY` row whose `vocabulary_id` is `None`, when that row exists.
    #[must_use]
    pub fn vocabulary_version(&self) -> Option<&str> {
        self.vocabulary_version.as_deref()
    }
}

/// The step of a table's load that failed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum LoadStep {
    /// Opening the table's transaction.
    Begin,
    /// Removing the rows the table held.
    Truncate,
    /// Starting `COPY`.
    Copy,
    /// Sending the file's rows.
    Send,
    /// Ending `COPY`, when PostgreSQL checks the rows.
    Finish,
    /// Committing the table's transaction.
    Commit,
}

impl fmt::Display for LoadStep {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Begin => "opening the transaction",
            Self::Truncate => "removing the rows it held",
            Self::Copy => "starting COPY",
            Self::Send => "sending the rows",
            Self::Finish => "ending COPY",
            Self::Commit => "committing the transaction",
        })
    }
}

/// A table [`load`] refused or PostgreSQL refused; that table keeps the rows
/// it held, and every table loaded before it keeps its new rows.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum LoadError {
    /// The file could not be read.
    #[error("{path} could not be read")]
    File {
        /// The file.
        path: PathBuf,
        /// What the file system reported.
        #[source]
        source: std::io::Error,
    },
    /// The file holds no header row.
    #[error("{path} holds no header row")]
    NoHeader {
        /// The file.
        path: PathBuf,
    },
    /// The header row names a column the table does not have, or names one
    /// twice.
    #[error("the header row of {path} names `{column}`, which is not a column of {table} once")]
    Column {
        /// The file.
        path: PathBuf,
        /// The table the file fills.
        table: &'static str,
        /// The column name as the header row spells it.
        column: String,
    },
    /// The file holds the byte the loader tells `COPY` is a quote.
    #[error("{path} holds a backspace byte, which no Athena download carries")]
    Quote {
        /// The file.
        path: PathBuf,
    },
    /// PostgreSQL refused a step.
    #[error("PostgreSQL refused {step} for {table} from {path}")]
    Refused {
        /// The table being loaded.
        table: &'static str,
        /// The file being loaded.
        path: PathBuf,
        /// The step PostgreSQL refused.
        step: LoadStep,
        /// What PostgreSQL reported.
        #[source]
        source: sqlx::Error,
    },
    /// The vocabulary version could not be read once the tables were loaded.
    #[error("PostgreSQL refused reading the vocabulary version")]
    Version {
        /// What PostgreSQL reported.
        #[source]
        source: sqlx::Error,
    },
}

/// Replaces the rows of every table of `export` in the schema `pool` binds.
///
/// Each table is loaded in a transaction of its own: its rows are removed
/// with `TRUNCATE`, which PostgreSQL refuses while a foreign key of another
/// table references it, and the file goes in through `COPY ... (FORMAT csv)`
/// with a tab delimiter, the header row's columns, and empty fields read as
/// null (<https://www.postgresql.org/docs/current/sql-copy.html>).
/// PostgreSQL reads a `yyyyMMdd` date as ISO 8601 in every `DateStyle`
/// (<https://www.postgresql.org/docs/current/datatype-datetime.html#DATATYPE-DATETIME-DATE-TABLE>),
/// and refuses a row whose field count disagrees with the header.
///
/// # Errors
///
/// Returns [`LoadError`] naming the first table that is refused; the tables
/// before it keep their new rows.
pub async fn load(pool: &CdmPool, export: &Export) -> Result<Loaded, LoadError> {
    let mut tables = Vec::new();
    for file in &export.files {
        tables.push((file.table.name, load_file(pool, file).await?));
    }
    let vocabulary_version = sqlx::query_scalar::<_, Option<String>>(
        "SELECT vocabulary_version FROM vocabulary WHERE vocabulary_id = 'None'",
    )
    .fetch_optional(pool.pool())
    .await
    .map_err(|source| LoadError::Version { source })?
    .flatten();
    Ok(Loaded {
        tables,
        vocabulary_version,
    })
}

/// Replaces the rows of the table `file` fills, and returns the rows written.
async fn load_file(pool: &CdmPool, file: &ExportFile) -> Result<u64, LoadError> {
    let table = file.table.name;
    let path = file.path.as_path();
    let unreadable = |source| LoadError::File {
        path: path.to_path_buf(),
        source,
    };
    let refused = |step| {
        move |source| LoadError::Refused {
            table,
            path: path.to_path_buf(),
            step,
            source,
        }
    };
    let opened = tokio::fs::File::open(path).await.map_err(unreadable)?;
    let mut reader = BufReader::with_capacity(CHUNK, opened);
    let mut header = Vec::new();
    reader
        .read_until(b'\n', &mut header)
        .await
        .map_err(unreadable)?;
    let columns = columns(file, &header)?;
    let statement = format!(
        "COPY {} ({}) FROM STDIN (FORMAT csv, DELIMITER E'\\t', QUOTE E'\\b', ENCODING 'UTF8')",
        quoted(table),
        columns
            .iter()
            .map(|column| quoted(column))
            .collect::<Vec<_>>()
            .join(", ")
    );
    let mut transaction = pool
        .pool()
        .begin()
        .await
        .map_err(refused(LoadStep::Begin))?;
    sqlx::query(sqlx::AssertSqlSafe(format!(
        "TRUNCATE TABLE {}",
        quoted(table)
    )))
    .execute(&mut *transaction)
    .await
    .map_err(refused(LoadStep::Truncate))?;
    let mut copy = transaction
        .copy_in_raw(&statement)
        .await
        .map_err(refused(LoadStep::Copy))?;
    let mut chunk = vec![0; CHUNK];
    loop {
        let read = match reader.read(&mut chunk).await {
            Ok(read) => read,
            Err(source) => {
                drop(copy.abort("the vocabulary file could not be read").await);
                return Err(unreadable(source));
            }
        };
        let Some(bytes) = chunk.get(..read) else {
            break;
        };
        if bytes.is_empty() {
            break;
        }
        if bytes.contains(&QUOTE) {
            drop(
                copy.abort("the vocabulary file holds a backspace byte")
                    .await,
            );
            return Err(LoadError::Quote {
                path: path.to_path_buf(),
            });
        }
        copy.send(bytes).await.map_err(refused(LoadStep::Send))?;
    }
    let rows = copy.finish().await.map_err(refused(LoadStep::Finish))?;
    transaction
        .commit()
        .await
        .map_err(refused(LoadStep::Commit))?;
    Ok(rows)
}

/// Returns the columns the header row `line` names, each as the CDM
/// definitions spell it.
fn columns(file: &ExportFile, line: &[u8]) -> Result<Vec<&'static str>, LoadError> {
    let line = line.strip_suffix(b"\n").unwrap_or(line);
    if line.is_empty() {
        return Err(LoadError::NoHeader {
            path: file.path.clone(),
        });
    }
    let text = String::from_utf8_lossy(line);
    let mut columns: Vec<&'static str> = Vec::new();
    for name in text.split('\t') {
        let column = file
            .table
            .column(&name.to_lowercase())
            .map(|column| column.name)
            .filter(|column| !columns.contains(column))
            .ok_or_else(|| LoadError::Column {
                path: file.path.clone(),
                table: file.table.name,
                column: name.to_owned(),
            })?;
        columns.push(column);
    }
    Ok(columns)
}

/// Returns `name` as a quoted PostgreSQL identifier; every name quoted here
/// is a table or a column of the generated metadata.
fn quoted(name: &str) -> String {
    format!("\"{name}\"")
}

#[cfg(test)]
mod tests {
    use super::{ExportFile, LoadError, columns};
    use crate::meta;
    use std::path::PathBuf;

    fn concept() -> ExportFile {
        ExportFile {
            table: meta::table("concept").expect("the concept table"),
            path: PathBuf::from("CONCEPT.csv"),
        }
    }

    #[test]
    fn a_header_row_names_columns_of_the_table_in_its_own_order() {
        let read = columns(&concept(), b"concept_code\tCONCEPT_ID\tconcept_name\n")
            .expect("the header reads");
        assert_eq!(vec!["concept_code", "concept_id", "concept_name"], read);
    }

    #[test]
    fn an_unknown_repeated_or_carriage_returned_column_is_refused() {
        for header in [
            &b"concept_id\tcolour\n"[..],
            b"concept_id\tconcept_id\n",
            b"concept_id\tconcept_name\r\n",
        ] {
            let refused = columns(&concept(), header).expect_err("the header is refused");
            assert!(matches!(refused, LoadError::Column { .. }), "{refused:?}");
        }
        assert!(matches!(
            columns(&concept(), b"\n"),
            Err(LoadError::NoHeader { .. })
        ));
    }
}
