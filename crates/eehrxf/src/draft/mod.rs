// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The data sets of the draft `EEHRxF` implementing act set beside the
//! Xt-EHR logical models the crate reads.
//!
//! The draft Commission Implementing Regulation on the European electronic
//! health record exchange format (Regulation (EU) 2025/327 Art 15(1), Have
//! your say initiative 17512, published 2026-11-23) fixes the metadata of
//! every document and data group in Annex III, the data set of each priority
//! category in Annexes IV to IX and the common data groups in Annex X. It
//! names no logical model package, and its element names and cardinalities
//! differ from the Xt-EHR models 1.0.0 in places. [`AnnexCrosswalk`] records,
//! element by element, where each annex row sits in those models and how the
//! two differ, so every divergence is a fact the component can act on. The
//! draft leads where the two differ.

use std::fmt;
use std::str::FromStr;

use crate::category::XTEHR_CANONICAL;
use crate::dataset::Cardinality;
use crate::dataset::DatasetModel;
use crate::dataset::Max;

/// The draft act the crosswalk reads, as its Have your say entry names it.
pub const DRAFT: &str = "EEHRxF draft 2026-11-23";

/// The crosswalk rows the crate ships, to parse into an [`AnnexCrosswalk`].
///
/// They hold the draft of 2026-11-23 against the Xt-EHR models 1.0.0, one row
/// per annex element and one per Xt-EHR element no annex names.
pub const SHIPPED: &str = include_str!("annexes.tsv");

/// The column header the shipped rows open with.
const HEADER: &str = "annex\ttable\tdraft model\tdraft element\tdraft cardinality\txtehr model\txtehr element\txtehr cardinality\tverdict";

/// An annex of the draft act that carries a data set.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Annex {
    /// Annex III, the metadata of every document and data group.
    Iii,
    /// Annex IV, patient summaries.
    Iv,
    /// Annex V, electronic prescriptions.
    V,
    /// Annex VI, electronic dispensations.
    Vi,
    /// Annex VII, medical imaging studies and related reports.
    Vii,
    /// Annex VIII, medical test results and related reports.
    Viii,
    /// Annex IX, discharge reports.
    Ix,
    /// Annex X, the common data groups.
    X,
}

impl Annex {
    /// Returns the roman numeral the act numbers the annex with.
    #[must_use]
    pub const fn numeral(self) -> &'static str {
        match self {
            Self::Iii => "III",
            Self::Iv => "IV",
            Self::V => "V",
            Self::Vi => "VI",
            Self::Vii => "VII",
            Self::Viii => "VIII",
            Self::Ix => "IX",
            Self::X => "X",
        }
    }

    /// Returns every annex that carries a data set, in the act's order.
    #[must_use]
    pub const fn all() -> [Self; 8] {
        [
            Self::Iii,
            Self::Iv,
            Self::V,
            Self::Vi,
            Self::Vii,
            Self::Viii,
            Self::Ix,
            Self::X,
        ]
    }
}

impl fmt::Display for Annex {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.numeral())
    }
}

impl FromStr for Annex {
    type Err = DraftError;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        Self::all()
            .into_iter()
            .find(|annex| annex.numeral() == text)
            .ok_or_else(|| DraftError::Annex(text.to_owned()))
    }
}

/// How an annex element and its Xt-EHR element relate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Verdict {
    /// The same name at the same place, with the same cardinality.
    Same,
    /// The Xt-EHR model holds the element under another name or place.
    Renamed,
    /// The same name at the same place, with another cardinality.
    CardinalityDiffers,
    /// The annex names an element the Xt-EHR model does not hold.
    MissingInXtEhr,
    /// The Xt-EHR model holds an element no annex row names.
    MissingInDraft,
}

impl Verdict {
    /// Returns the verdict as the shipped rows spell it.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Same => "same",
            Self::Renamed => "renamed",
            Self::CardinalityDiffers => "cardinality differs",
            Self::MissingInXtEhr => "missing in Xt-EHR",
            Self::MissingInDraft => "missing in the draft",
        }
    }
}

impl fmt::Display for Verdict {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for Verdict {
    type Err = DraftError;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        [
            Self::Same,
            Self::Renamed,
            Self::CardinalityDiffers,
            Self::MissingInXtEhr,
            Self::MissingInDraft,
        ]
        .into_iter()
        .find(|verdict| verdict.as_str() == text)
        .ok_or_else(|| DraftError::Verdict(text.to_owned()))
    }
}

/// One row of the crosswalk.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    annex: Annex,
    table: Option<u8>,
    draft_model: String,
    draft_element: Option<String>,
    draft_cardinality: Option<Cardinality>,
    xtehr_model: String,
    xtehr_element: Option<String>,
    xtehr_cardinality: Option<Cardinality>,
    verdict: Verdict,
}

impl Row {
    /// Returns the annex the row belongs to.
    #[must_use]
    pub const fn annex(&self) -> Annex {
        self.annex
    }

    /// Returns the number of the annex table, for an annex with more than
    /// one.
    #[must_use]
    pub const fn table(&self) -> Option<u8> {
        self.table
    }

    /// Returns the name the annex gives its data set: the root of the
    /// table, or the table title for Annex III.
    #[must_use]
    pub fn draft_model(&self) -> &str {
        &self.draft_model
    }

    /// Returns the element path below the annex root, `None` for the row of
    /// the root itself and for an element no annex row names.
    #[must_use]
    pub fn draft_element(&self) -> Option<&str> {
        self.draft_element.as_deref()
    }

    /// Returns the cardinality the annex gives the element.
    #[must_use]
    pub const fn draft_cardinality(&self) -> Option<Cardinality> {
        self.draft_cardinality
    }

    /// Returns the name of the Xt-EHR logical model.
    #[must_use]
    pub fn xtehr_model(&self) -> &str {
        &self.xtehr_model
    }

    /// Returns the canonical URL of the Xt-EHR logical model.
    #[must_use]
    pub fn xtehr_url(&self) -> String {
        model_url(&self.xtehr_model)
    }

    /// Returns the Xt-EHR element path below the model root, `None` for the
    /// root row and for an annex element the model does not hold.
    #[must_use]
    pub fn xtehr_element(&self) -> Option<&str> {
        self.xtehr_element.as_deref()
    }

    /// Returns the full Xt-EHR element path, the model name included.
    #[must_use]
    pub fn xtehr_path(&self) -> String {
        match &self.xtehr_element {
            Some(element) => format!("{}.{element}", self.xtehr_model),
            None => self.xtehr_model.clone(),
        }
    }

    /// Returns the cardinality the Xt-EHR model gives the element.
    #[must_use]
    pub const fn xtehr_cardinality(&self) -> Option<Cardinality> {
        self.xtehr_cardinality
    }

    /// Returns how the two elements relate.
    #[must_use]
    pub const fn verdict(&self) -> Verdict {
        self.verdict
    }

    /// Returns whether the row is the root of an annex data set.
    #[must_use]
    pub fn is_root(&self) -> bool {
        self.draft_element.is_none() && self.xtehr_element.is_none()
    }

    /// Returns the verdict the row's own cells imply, which a shipped row
    /// must carry.
    #[must_use]
    pub fn implied_verdict(&self) -> Verdict {
        if self.is_root() {
            return if self.draft_model == self.xtehr_model {
                Verdict::Same
            } else {
                Verdict::Renamed
            };
        }
        let (Some(draft), Some(xtehr)) = (&self.draft_element, &self.xtehr_element) else {
            return if self.draft_element.is_none() {
                Verdict::MissingInDraft
            } else {
                Verdict::MissingInXtEhr
            };
        };
        let expected = match implied_prefix(self.annex) {
            Some(prefix) => format!("{prefix}.{draft}"),
            None => draft.clone(),
        };
        if *xtehr != expected {
            Verdict::Renamed
        } else if self.draft_cardinality == self.xtehr_cardinality {
            Verdict::Same
        } else {
            Verdict::CardinalityDiffers
        }
    }
}

/// The canonical URL of the Xt-EHR logical model `name`.
fn model_url(name: &str) -> String {
    format!("{XTEHR_CANONICAL}/StructureDefinition/{name}")
}

/// The element of the Xt-EHR model an Annex III table spells out: its rows
/// are the header of a document or of a data group.
const fn implied_prefix(annex: Annex) -> Option<&'static str> {
    match annex {
        Annex::Iii => Some("header"),
        _ => None,
    }
}

/// The crosswalk of the draft annexes to the Xt-EHR logical models.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnnexCrosswalk {
    rows: Vec<Row>,
}

/// Parses crosswalk rows from tab-separated text with the shipped header
/// line, refusing with a [`DraftError`] the first line that is not a row: a
/// wrong header, a wrong number of cells, an unknown annex, table,
/// cardinality or verdict, or a verdict the row's cells contradict.
impl FromStr for AnnexCrosswalk {
    type Err = DraftError;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        let mut lines = text.lines().enumerate();
        match lines.next() {
            Some((_, header)) if header == HEADER => {}
            _ => return Err(DraftError::Header),
        }
        let rows = lines
            .map(|(index, line)| {
                parse_row(line).map_err(|error| DraftError::Line(index + 1, Box::new(error)))
            })
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Self { rows })
    }
}

impl AnnexCrosswalk {
    /// Returns every row, in the order of the annexes and their tables.
    #[must_use]
    pub fn rows(&self) -> &[Row] {
        &self.rows
    }

    /// Returns the rows of one annex.
    pub fn annex(&self, annex: Annex) -> impl Iterator<Item = &Row> {
        self.rows.iter().filter(move |row| row.annex == annex)
    }

    /// Returns the rows with one verdict.
    pub fn with_verdict(&self, verdict: Verdict) -> impl Iterator<Item = &Row> {
        self.rows.iter().filter(move |row| row.verdict == verdict)
    }

    /// Returns the row of the annex element at `element` below the root
    /// `draft_model`.
    #[must_use]
    pub fn draft(&self, draft_model: &str, element: &str) -> Option<&Row> {
        self.rows.iter().find(|row| {
            row.draft_model == draft_model && row.draft_element.as_deref() == Some(element)
        })
    }

    /// Holds the rows to the Xt-EHR models in `model`: every Xt-EHR element
    /// a row names exists at the cardinality the row records, and every
    /// element of a model the crosswalk reads has a row.
    #[must_use]
    pub fn check_xtehr(&self, model: &DatasetModel) -> Vec<Discrepancy> {
        let mut found = Vec::new();
        let mut models: Vec<&str> = self
            .rows
            .iter()
            .map(|row| row.xtehr_model.as_str())
            .collect();
        models.sort_unstable();
        models.dedup();
        for name in models {
            let url = model_url(name);
            let Some(logical) = model.model(&url) else {
                found.push(Discrepancy::NoModel(name.to_owned()));
                continue;
            };
            // An Annex III table is the implied element itself.
            let implied = self
                .rows
                .iter()
                .filter(|row| row.xtehr_model == name)
                .find_map(|row| implied_prefix(row.annex))
                .map(|prefix| format!("{name}.{prefix}"));
            for element in logical.elements() {
                let path = element.path().as_str();
                if implied.as_deref() != Some(path)
                    && !self
                        .rows
                        .iter()
                        .any(|row| row.xtehr_model == name && row.xtehr_path() == path)
                {
                    found.push(Discrepancy::Unlisted(path.to_owned()));
                }
            }
            for row in self.rows.iter().filter(|row| row.xtehr_model == name) {
                if row.verdict == Verdict::MissingInXtEhr {
                    continue;
                }
                let path = row.xtehr_path();
                match logical.element(&path) {
                    None => found.push(Discrepancy::NoElement(path)),
                    Some(element) if Some(element.cardinality()) != row.xtehr_cardinality => {
                        found.push(Discrepancy::Cardinality {
                            path,
                            recorded: row
                                .xtehr_cardinality
                                .map_or_else(String::new, |c| c.to_string()),
                            package: element.cardinality().to_string(),
                        });
                    }
                    Some(_) => {}
                }
            }
        }
        found
    }

    /// Holds the rows to the extracted text of the annexes: in each annex
    /// table, the cardinalities the text spells number exactly the rows that
    /// carry a draft cardinality, and the last step of every element path
    /// appears in that table's text.
    #[must_use]
    pub fn check_annexes(&self, text: &str) -> Vec<Discrepancy> {
        let sections = sections(text);
        let mut found = Vec::new();
        let mut keys: Vec<(Annex, Option<u8>)> =
            self.rows.iter().map(|row| (row.annex, row.table)).collect();
        keys.dedup();
        for (annex, table) in keys {
            let Some(section) = sections
                .iter()
                .find(|s| s.annex == annex && s.table == table)
            else {
                found.push(Discrepancy::NoSection(annex, table));
                continue;
            };
            let words: Vec<&str> = section
                .text
                .lines()
                .filter_map(|line| {
                    line.split_whitespace()
                        .find(|word| word.chars().any(|c| c != '+'))
                })
                .collect();
            let rows: Vec<&Row> = self
                .rows
                .iter()
                .filter(|row| row.annex == annex && row.table == table)
                .collect();
            let recorded = rows
                .iter()
                .filter(|row| row.draft_cardinality.is_some())
                .count();
            let spelled = cardinalities(&section.text);
            if recorded != spelled {
                found.push(Discrepancy::RowCount {
                    annex,
                    table,
                    recorded,
                    spelled,
                });
            }
            for row in rows {
                let Some(element) = &row.draft_element else {
                    continue;
                };
                let step = element.rsplit('.').next().unwrap_or(element);
                if !spells(&words, step) {
                    found.push(Discrepancy::NotInText(annex, table, element.clone()));
                }
            }
        }
        found
    }
}

/// A disagreement between the crosswalk and one of its sources.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Discrepancy {
    /// The package holds no logical model of this name.
    NoModel(String),
    /// The package holds this element and no row names it.
    Unlisted(String),
    /// A row names this element and the package does not hold it.
    NoElement(String),
    /// A row records another cardinality than the package gives.
    Cardinality {
        /// The element path.
        path: String,
        /// The cardinality the row records.
        recorded: String,
        /// The cardinality the package gives.
        package: String,
    },
    /// The annex text holds no table the rows name.
    NoSection(Annex, Option<u8>),
    /// The rows of a table and the cardinalities its text spells differ in
    /// number.
    RowCount {
        /// The annex.
        annex: Annex,
        /// The table.
        table: Option<u8>,
        /// The rows with a draft cardinality.
        recorded: usize,
        /// The cardinalities the text spells.
        spelled: usize,
    },
    /// The text of a table does not name this element.
    NotInText(Annex, Option<u8>, String),
}

impl fmt::Display for Discrepancy {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let table = |table: &Option<u8>| table.map_or_else(String::new, |n| format!(" Table {n}"));
        match self {
            Self::NoModel(name) => write!(f, "the package holds no logical model {name}"),
            Self::Unlisted(path) => write!(f, "{path} is in the package and in no row"),
            Self::NoElement(path) => write!(f, "{path} is in a row and not in the package"),
            Self::Cardinality {
                path,
                recorded,
                package,
            } => {
                write!(
                    f,
                    "{path}: the row records {recorded}, the package gives {package}"
                )
            }
            Self::NoSection(annex, t) => {
                write!(f, "the annex text holds no Annex {annex}{}", table(t))
            }
            Self::RowCount {
                annex,
                table: t,
                recorded,
                spelled,
            } => write!(
                f,
                "Annex {annex}{}: {recorded} rows carry a cardinality, the text spells {spelled}",
                table(t)
            ),
            Self::NotInText(annex, t, element) => {
                write!(f, "Annex {annex}{} does not name {element}", table(t))
            }
        }
    }
}

/// Why crosswalk rows could not be read.
#[derive(Debug, thiserror::Error)]
pub enum DraftError {
    /// The text does not open with the shipped header line.
    #[error("the crosswalk does not open with its header line")]
    Header,
    /// A line is not a row.
    #[error("crosswalk line {0}: {1}")]
    Line(usize, #[source] Box<DraftError>),
    /// A row does not hold nine cells.
    #[error("a row holds {0} cells, not 9")]
    Cells(usize),
    /// An annex numeral the crosswalk does not know.
    #[error("no annex {0} carries a data set")]
    Annex(String),
    /// A table number that is not a number.
    #[error("{text:?} is no table number")]
    Table {
        /// The cell.
        text: String,
        /// Why it is no number.
        #[source]
        source: std::num::ParseIntError,
    },
    /// A cardinality that is not `min..max`.
    #[error("{0:?} is no cardinality")]
    Cardinality(String),
    /// A cardinality bound that is not a number.
    #[error("{text:?} is no cardinality bound")]
    Bound {
        /// The cell.
        text: String,
        /// Why it is no number.
        #[source]
        source: std::num::ParseIntError,
    },
    /// A verdict the crosswalk does not know.
    #[error("{0:?} is no verdict")]
    Verdict(String),
    /// A verdict the row's cells contradict.
    #[error("the row says {recorded}, its cells imply {implied}")]
    Contradicted {
        /// The verdict the row carries.
        recorded: Verdict,
        /// The verdict its cells imply.
        implied: Verdict,
    },
}

fn parse_row(line: &str) -> Result<Row, DraftError> {
    let cells: Vec<&str> = line.split('\t').collect();
    let [
        annex,
        table,
        draft_model,
        draft_element,
        draft_card,
        xtehr_model,
        xtehr_element,
        xtehr_card,
        verdict,
    ] = cells.as_slice()
    else {
        return Err(DraftError::Cells(cells.len()));
    };
    let row = Row {
        annex: annex.parse()?,
        table: if table.is_empty() {
            None
        } else {
            Some(table.parse().map_err(|source| DraftError::Table {
                text: (*table).to_owned(),
                source,
            })?)
        },
        draft_model: (*draft_model).to_owned(),
        draft_element: non_empty(draft_element),
        draft_cardinality: cardinality(draft_card)?,
        xtehr_model: (*xtehr_model).to_owned(),
        xtehr_element: non_empty(xtehr_element),
        xtehr_cardinality: cardinality(xtehr_card)?,
        verdict: verdict.parse()?,
    };
    let implied = row.implied_verdict();
    if implied != row.verdict {
        return Err(DraftError::Contradicted {
            recorded: row.verdict,
            implied,
        });
    }
    Ok(row)
}

fn non_empty(cell: &str) -> Option<String> {
    (!cell.is_empty()).then(|| cell.to_owned())
}

fn cardinality(cell: &str) -> Result<Option<Cardinality>, DraftError> {
    if cell.is_empty() {
        return Ok(None);
    }
    let refused = || DraftError::Cardinality(cell.to_owned());
    let (min, max) = cell.split_once("..").ok_or_else(refused)?;
    let bound = |source| DraftError::Bound {
        text: cell.to_owned(),
        source,
    };
    let min = min.parse().map_err(bound)?;
    let max = if max == "*" {
        Max::Unbounded
    } else {
        Max::Bounded(max.parse().map_err(bound)?)
    };
    Ok(Some(Cardinality::new(min, max)))
}

/// One annex table of the extracted text.
struct Section {
    annex: Annex,
    table: Option<u8>,
    text: String,
}

/// Splits the extracted annex text at each annex heading and each table
/// title of an annex that carries a data set; the text of Annexes I and II
/// belongs to no section.
fn sections(text: &str) -> Vec<Section> {
    let mut sections: Vec<Section> = Vec::new();
    for line in text.lines() {
        let trimmed = line.trim_start();
        if let Some(rest) = trimmed.strip_prefix("ANNEX ") {
            let numeral = rest.split_whitespace().next().unwrap_or_default();
            match numeral.parse::<Annex>() {
                Ok(annex) => {
                    sections.push(Section {
                        annex,
                        table: None,
                        text: String::new(),
                    });
                    continue;
                }
                Err(_) if ["I", "II"].contains(&numeral) => {
                    sections.clear();
                    continue;
                }
                Err(_) => {}
            }
        }
        let open = sections.last().map(|section| section.annex);
        let number = trimmed
            .strip_prefix("Table ")
            .and_then(|rest| rest.split_whitespace().next())
            .and_then(|word| word.parse::<u8>().ok());
        if let (Some(annex), Some(table)) = (open, number) {
            sections.push(Section {
                annex,
                table: Some(table),
                text: String::new(),
            });
            continue;
        }
        if let Some(section) = sections.last_mut() {
            section.text.push_str(line);
            section.text.push('\n');
        }
    }
    sections
}

/// Whether the leading words of the lines, in order, spell `name`, which the
/// annex text needs where a table wraps a long name over its lines; a word
/// after the first continues the name only when it is at least three
/// characters long or ends it.
fn spells(words: &[&str], name: &str) -> bool {
    (0..words.len()).any(|start| {
        let mut rest = name;
        for word in words.iter().skip(start) {
            if rest.is_empty() {
                break;
            }
            let first = rest.len() == name.len();
            match rest.strip_prefix(word) {
                Some(after) if first || after.is_empty() || word.len() >= 3 => rest = after,
                _ if first => return false,
                _ => {}
            }
        }
        rest.is_empty()
    })
}

/// The number of cardinalities (`0..1`, `1..*`) a text spells.
fn cardinalities(text: &str) -> usize {
    text.split(|c: char| c.is_whitespace() || c == ',' || c == '(' || c == ')')
        .filter(|token| cardinality(token).ok().flatten().is_some())
        .count()
}
