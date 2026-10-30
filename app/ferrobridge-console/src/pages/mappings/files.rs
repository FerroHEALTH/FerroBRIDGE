// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The Files tab of the Mapping set: every file of the set, with its find
//! field, its language and owner filters and its pager.

use std::borrow::Cow;

use ferrobridge_operator_api::operator::{Language, Listed, Owner};
use ferrohealth_ui::html::Html;
use ferrohealth_ui::i18n::Locale;
use ferrohealth_ui::icon::Glyph;
use ferrohealth_ui::query::Query;
use ferrohealth_ui::table::filter::{
    FilterChoice, FilterMenu, Find, MenuForm, filter_menu, find_form,
};
use ferrohealth_ui::table::model::{Paging, StateSource, Window};
use ferrohealth_ui::table::pager::{Listing, link_pager};
use ferrohealth_ui::table::view::{
    Body, Column, DataTable, Role, Sorting, cell, data_table, title_cell,
};
use ferrohealth_ui::view::template::list::{ActiveFilters, FilterChip, active_filters};

use crate::locale::messages::Msg;
use crate::locale::{tr, tr_with};
use crate::markup::{code, h, t};
use crate::pages::mappings::count;
use crate::pages::mappings::tab_href;
use crate::pages::parts::{
    diagnostics_state, file_href, language_name, owner_tag, owner_word, quiet, tally,
};

/// The languages the Files tab filters by, in the order its menu lists them.
const LANGUAGES: [Language; 2] = [Language::Fhirconnect, Language::Omocl];

/// The owners the Files tab filters by, in the order its menu lists them.
const OWNERS: [Owner; 2] = [Owner::Provisioned, Owner::Console];

/// The filters of the Files tab, as its query carries them.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct FileFilters {
    /// The words of the find field.
    words: String,
    /// The languages kept; none while every language is.
    languages: Vec<Language>,
    /// The owners kept; none while every owner is.
    owners: Vec<Owner>,
}

impl FileFilters {
    /// The filters `query` names; a value that names nothing is no filter.
    fn read(query: &Query) -> Self {
        Self {
            words: query.get_str("q").unwrap_or_default().trim().to_owned(),
            languages: kept(query, "language", &LANGUAGES, language_key),
            owners: kept(query, "owner", &OWNERS, owner_key),
        }
    }

    /// Whether `file` passes every filter.
    fn keeps(&self, file: &Listed) -> bool {
        (self.languages.is_empty() || self.languages.contains(&file.language))
            && (self.owners.is_empty() || self.owners.contains(&file.owner))
            && (self.words.is_empty()
                || file
                    .path
                    .to_lowercase()
                    .contains(&self.words.to_lowercase()))
    }

    /// The query of these filters, `without` one of them, with `paging`.
    fn href(&self, without: Option<&str>, paging: Option<Paging>) -> String {
        let mut pairs = Vec::new();
        if !self.words.is_empty() && without != Some("q") {
            pairs.push(format!("q={}", encoded(&self.words)));
        }
        if without != Some("language") {
            pairs.extend(
                self.languages
                    .iter()
                    .map(|language| format!("language={}", language_key(*language))),
            );
        }
        if without != Some("owner") {
            pairs.extend(
                self.owners
                    .iter()
                    .map(|owner| format!("owner={}", owner_key(*owner))),
            );
        }
        if let Some(paging) = paging {
            pairs.push(format!("page={}&size={}", paging.page, paging.size));
        }
        let base = tab_href("files");
        if pairs.is_empty() {
            base
        } else {
            format!("{base}?{}", pairs.join("&"))
        }
    }
}

/// The choices of `all` the query names under `key`, by `key_of`: none
/// when it names every choice or none of them, since both keep every row.
fn kept<T: Copy + PartialEq>(
    query: &Query,
    key: &str,
    all: &[T],
    key_of: fn(T) -> &'static str,
) -> Vec<T> {
    let named = query
        .pairs()
        .iter()
        .filter(|(name, _)| name == key)
        .map(|(_, value)| value)
        .collect::<Vec<_>>();
    let kept = all
        .iter()
        .copied()
        .filter(|choice| named.iter().any(|name| *name == key_of(*choice)))
        .collect::<Vec<_>>();
    if kept.len() == all.len() {
        Vec::new()
    } else {
        kept
    }
}

/// The query value of `language`.
const fn language_key(language: Language) -> &'static str {
    match language {
        Language::Fhirconnect => "fhirconnect",
        Language::Omocl => "omocl",
    }
}

/// The query value of `owner`.
const fn owner_key(owner: Owner) -> &'static str {
    match owner {
        Owner::Provisioned => "provisioned",
        Owner::Console => "console",
    }
}

/// `words` encoded for a URL query (RFC 3986 §3.4).
fn encoded(words: &str) -> String {
    url::form_urlencoded::byte_serialize(words.as_bytes()).collect()
}

/// The Files tab's toolbar: the find field and the Language and Owner
/// filter menus, whose checkboxes its form sends with the find words, each
/// choice counting the files it keeps among `listing` under the other
/// filters.
fn files_toolbar(l: &Locale, filters: &FileFilters, listing: &[Listed]) -> Html {
    let counted = |with: &FileFilters| {
        u64::try_from(listing.iter().filter(|file| with.keeps(file)).count()).unwrap_or(u64::MAX)
    };
    let languages = LANGUAGES
        .into_iter()
        .map(|language| {
            let with = FileFilters {
                languages: vec![language],
                ..filters.clone()
            };
            FilterChoice::new(
                language_key(language),
                language_name(language),
                counted(&with),
                filters.languages.is_empty() || filters.languages.contains(&language),
            )
        })
        .collect();
    let owners = OWNERS
        .into_iter()
        .map(|owner| {
            let with = FileFilters {
                owners: vec![owner],
                ..filters.clone()
            };
            FilterChoice::new(
                owner_key(owner),
                owner_word(l, owner),
                counted(&with),
                filters.owners.is_empty() || filters.owners.contains(&owner),
            )
        })
        .collect();
    let language_menu = filter_menu(
        l,
        FilterMenu {
            id: "filter-language",
            name: "language",
            glyph: Glyph::LIST_FILTER,
            label: tr(l, Msg::ColLanguage).into(),
            title: tr(l, Msg::FilterShowLanguages),
            choices: languages,
            note: None,
            form: MenuForm::Enclosing,
        },
    );
    let owner_menu = filter_menu(
        l,
        FilterMenu {
            id: "filter-owner",
            name: "owner",
            glyph: Glyph::LIST_FILTER,
            label: tr(l, Msg::ColOwner).into(),
            title: tr(l, Msg::FilterShowOwners),
            choices: owners,
            note: None,
            form: MenuForm::Enclosing,
        },
    );
    find_form(
        Find {
            id: "find-files",
            name: "q",
            label: &tr(l, Msg::FindFiles),
            value: &filters.words,
            source: StateSource::Url,
            action: None,
            fields: &[],
        },
        language_menu.then(owner_menu),
    )
}

/// The Files tab's active filter chips, `total` of `all` files shown, or
/// nothing while no filter is set.
fn files_chips(l: &Locale, filters: &FileFilters, total: u32, all: usize) -> Option<Html> {
    let mut chips = Vec::new();
    if !filters.languages.is_empty() {
        let value = filters
            .languages
            .iter()
            .map(|language| language_name(*language))
            .collect::<Vec<_>>()
            .join(", ");
        chips.push(FilterChip {
            label: tr_with(l, Msg::ChipLanguage, &[("value", value.into())]),
            remove: filters.href(Some("language"), None),
        });
    }
    if !filters.owners.is_empty() {
        let value = filters
            .owners
            .iter()
            .map(|owner| owner_word(l, *owner))
            .collect::<Vec<_>>()
            .join(", ");
        chips.push(FilterChip {
            label: tr_with(l, Msg::ChipOwner, &[("value", value.into())]),
            remove: filters.href(Some("owner"), None),
        });
    }
    if !filters.words.is_empty() {
        chips.push(FilterChip {
            label: tr_with(l, Msg::ChipPath, &[("value", filters.words.clone().into())]),
            remove: filters.href(Some("q"), None),
        });
    }
    (!chips.is_empty()).then(|| {
        active_filters(
            l,
            ActiveFilters {
                chips,
                clear: tab_href("files"),
                shown: Some(u64::from(total)),
            },
            Some(count(all)),
        )
    })
}

/// The Files tab: the find field and the filters, the files on the page,
/// and the pager.
pub(super) fn files_body(l: &Locale, query: &Query, listing: &[Listed]) -> (Html, Option<Html>) {
    let filters = FileFilters::read(query);
    let mut kept = listing
        .iter()
        .filter(|file| filters.keeps(file))
        .collect::<Vec<_>>();
    kept.sort_by(|a, b| a.path.cmp(&b.path));
    let total = u32::try_from(kept.len()).unwrap_or(u32::MAX);
    let paging = Paging::from_query(query);
    let window = Window::over(total, paging);
    let shown = window.slice(&kept);
    let toolbar = files_toolbar(l, &filters, listing);
    let chips = files_chips(l, &filters, total, listing.len()).unwrap_or_default();
    let table = if shown.is_empty() {
        quiet(
            tr(l, Msg::NoFileMatches),
            Some((tr(l, Msg::ClearFilters), tab_href("files"))),
        )
    } else {
        files_table(l, shown)
    };
    let body = h(format!(
        r#"<div class="fui-panel-body fui-stack">{toolbar}{chips}</div>{table}"#
    ));
    let pager = (total > 0).then(|| {
        let href = |paging: Paging| filters.href(None, Some(paging));
        link_pager(
            l,
            Listing::new(Cow::Owned(tr(l, Msg::TabFiles))),
            window,
            &href,
        )
    });
    (body, pager)
}

/// The files on the page.
fn files_table(l: &Locale, files: &[&Listed]) -> Html {
    let columns = [
        Column::text("path", tr(l, Msg::ColPath)).role(Role::Title),
        Column::text("kind", tr(l, Msg::ColKind)).role(Role::Sub),
        Column::text("owner", tr(l, Msg::ColOwner)).role(Role::Wide),
        Column::text("diagnostics", tr(l, Msg::ColDiagnostics)),
    ];
    let [_, kind, owner, word] = &columns;
    let rows = files
        .iter()
        .map(|file| {
            let (errors, warnings) = tally(&file.diagnostics);
            h(format!(
                "<tr>{}{}{}{}</tr>",
                title_cell(Some(&file_href(&file.path)), code(&file.path), None),
                cell(
                    kind,
                    t(file.kind.clone().unwrap_or_else(|| String::from("–")))
                ),
                cell(owner, owner_tag(l, file.owner)),
                cell(
                    word,
                    diagnostics_state(l, errors, warnings, Msg::StateNoDiagnostic)
                ),
            ))
        })
        .collect::<Html>();
    data_table(DataTable {
        caption: &tr(l, Msg::TabFiles),
        columns: &columns,
        sorting: Sorting::default(),
        body: Body::Rows(rows),
    })
}
