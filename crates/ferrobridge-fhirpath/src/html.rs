// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The narrative rules `htmlChecks()` tests (FHIR R4
//! <https://hl7.org/fhir/R4/narrative.html#Narrative>).
//!
//! The div "SHALL contain only the basic HTML formatting elements described
//! in chapters 7-11 (except section 4 of chapter 9) and 15 of the HTML 4.0
//! standard, `<a>` elements (either name or href), images and internally
//! contained style attributes", and no head, body, external stylesheet,
//! deprecated element, script, form, base, link, frame, object or event
//! attribute; it SHALL have some non-whitespace content (`txt-2`).

use quick_xml::Reader;
use quick_xml::events::{BytesStart, Event};

/// The XHTML namespace the div is in.
const XHTML: &str = "http://www.w3.org/1999/xhtml";

// NOTE: R4 publishes no machine-readable list; no specification governs this: our own
// design, the elements and non-deprecated attributes of HTML 4.0 chapters 7, 8, 9.1 to
// 9.3, 10, 11 and 15, with `a` and `img`, read from the HTML 4.0 element and attribute index.
/// The elements a narrative may use.
const ELEMENTS: [&str; 50] = [
    "div",
    "span",
    "h1",
    "h2",
    "h3",
    "h4",
    "h5",
    "h6",
    "address",
    "bdo",
    "em",
    "strong",
    "dfn",
    "code",
    "samp",
    "kbd",
    "var",
    "cite",
    "abbr",
    "acronym",
    "blockquote",
    "q",
    "sub",
    "sup",
    "p",
    "br",
    "pre",
    "ul",
    "ol",
    "li",
    "dl",
    "dt",
    "dd",
    "table",
    "caption",
    "thead",
    "tfoot",
    "tbody",
    "colgroup",
    "col",
    "tr",
    "th",
    "td",
    "tt",
    "i",
    "b",
    "big",
    "small",
    "hr",
    "a",
];

/// The attributes a narrative element may carry, wherever HTML 4.0 admits
/// them on the elements above.
const ATTRIBUTES: [&str; 33] = [
    "id",
    "class",
    "style",
    "title",
    "lang",
    "dir",
    "name",
    "href",
    "src",
    "alt",
    "width",
    "height",
    "cite",
    "summary",
    "border",
    "frame",
    "rules",
    "cellspacing",
    "cellpadding",
    "span",
    "align",
    "char",
    "charoff",
    "valign",
    "abbr",
    "axis",
    "headers",
    "scope",
    "rowspan",
    "colspan",
    "xmlns",
    "xml:lang",
    "longdesc",
];

/// Whether an element and its attributes are admitted; the root must be a
/// `div` in the XHTML namespace.
fn admitted(start: &BytesStart<'_>, root: bool) -> bool {
    let name = start.name();
    let name = name.as_ref();
    if !(ELEMENTS.contains(&name) || name == "img") || (root && name != "div") {
        return false;
    }
    let mut namespaced = false;
    for attribute in start.attributes() {
        let Ok(attribute) = attribute else {
            return false;
        };
        let key = attribute.key.as_ref();
        if key == "xmlns" {
            namespaced = attribute.value.as_ref() == XHTML;
        } else if !ATTRIBUTES.contains(&key) {
            return false;
        }
    }
    !root || namespaced
}

/// Whether `xhtml`, a narrative div, meets the R4 narrative rules.
#[must_use]
pub(crate) fn check(xhtml: &str) -> bool {
    let mut reader = Reader::from_str(xhtml);
    let mut depth: usize = 0;
    let mut seen_root = false;
    let mut content = false;
    loop {
        match reader.read_event() {
            Err(_) | Ok(Event::DocType(_) | Event::PI(_)) => return false,
            Ok(Event::Eof) => return seen_root && depth == 0 && content,
            Ok(Event::Start(start)) => {
                if (seen_root && depth == 0) || !admitted(&start, depth == 0) {
                    return false;
                }
                seen_root = true;
                depth = depth.saturating_add(1);
            }
            Ok(Event::Empty(start)) => {
                if (seen_root && depth == 0) || !admitted(&start, depth == 0) {
                    return false;
                }
                seen_root = true;
                content |= start.name().as_ref() == "img";
            }
            Ok(Event::End(_)) => depth = depth.saturating_sub(1),
            Ok(Event::Text(text)) => {
                let blank = text.into_inner().chars().all(char::is_whitespace);
                if depth == 0 && !blank {
                    return false;
                }
                content |= !blank;
            }
            Ok(Event::CData(text)) => {
                content |= !text.into_inner().chars().all(char::is_whitespace);
            }
            Ok(Event::GeneralRef(_)) => content |= depth > 0,
            Ok(Event::Comment(_) | Event::Decl(_)) => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::check;

    #[test]
    fn a_plain_narrative_passes() {
        assert!(check(
            r#"<div xmlns="http://www.w3.org/1999/xhtml">This is a <b>simple</b> example</div>"#
        ));
    }

    #[test]
    fn scripts_events_and_forms_fail() {
        assert!(!check(
            r#"<div xmlns="http://www.w3.org/1999/xhtml"><script>x()</script>text</div>"#
        ));
        assert!(!check(
            r#"<div xmlns="http://www.w3.org/1999/xhtml"><p onclick="x()">text</p></div>"#
        ));
        assert!(!check(
            r#"<div xmlns="http://www.w3.org/1999/xhtml"><form>text</form></div>"#
        ));
        assert!(!check(
            r#"<div xmlns="http://www.w3.org/1999/xhtml"><font>text</font></div>"#
        ));
    }

    #[test]
    fn an_empty_or_blank_div_fails() {
        assert!(!check(
            r#"<div xmlns="http://www.w3.org/1999/xhtml">   </div>"#
        ));
        assert!(!check(r#"<div xmlns="http://www.w3.org/1999/xhtml"/>"#));
    }

    #[test]
    fn the_root_must_be_an_xhtml_div() {
        assert!(!check("<div>text</div>"));
        assert!(!check(
            r#"<span xmlns="http://www.w3.org/1999/xhtml">text</span>"#
        ));
        assert!(!check(
            r#"<div xmlns="http://www.w3.org/1999/xhtml">text</div><div/>"#
        ));
    }

    #[test]
    fn malformed_xml_fails() {
        assert!(!check(
            r#"<div xmlns="http://www.w3.org/1999/xhtml"><p>text</div>"#
        ));
    }
}
