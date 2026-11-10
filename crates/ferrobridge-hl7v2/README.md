<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# ferrobridge-hl7v2

The FerroBRIDGE HL7 v2 face: MLLP framing, MSH-18 decoding, positional
parsing over the generated v2 tables, the acknowledgment, and the
v2-to-FHIR ConceptMap interpreter.

Part of [FerroBRIDGE](https://ferrobridge.eu), a pure-Rust bridge between
openEHR and two interoperability targets: HL7 FHIR through the FHIRconnect
specification, and the OMOP Common Data Model through the OMOCL specification.

## Requiring the crate

The crate follows Cargo's SemVer rules: a compatible change moves the patch,
and a breaking change moves the minor while the crate is 0.x. Require it with a
caret range on its line, so one release serves every crate in your build that
uses it:

```toml
[dependencies]
ferrobridge-hl7v2 = "0.6"
```

Name a patch (`"0.5.N"`) when you need the first release that carries what
you use; the range is the same caret range.

## What it does

- `mllp`: the MLLP Release 1 frame codec (`<SB> message <EB><CR>`) on the
  `tokio-util` codec traits, and a listener that answers each frame through a
  handler and drains on a shutdown signal. A frame with no start block, a
  start block inside it, an end block without its carriage return, or a size
  past the ceiling is refused, and the handler may answer it with `AR` before
  the connection closes.
  `send` is the other side: it connects within a connect timeout, writes one
  frame and returns the reply frame within a reply timeout, with a typed error
  for each way the exchange fails.
- `decode`: the character set MSH-18 names (HL7 table 0211): ASCII, ISO 8859
  parts 1 to 9 and 15, and UTF-8, with a per-connection default for an empty
  MSH-18. A byte outside the declared set is refused, never replaced.
- `parse`: segments, fields, repetitions, components and subcomponents by the
  MSH-1 and MSH-2 delimiters, the escape sequences decoded, and the segments
  placed in the segment-group tree of the message structure from
  [`hl7v2-types`](https://docs.rs/hl7v2-types). A Z-segment, a segment out of
  place and a field past its segment's table are counted outcomes; a missing
  required segment or field is a refusal.
- `ack`: the original-mode acknowledgment, `AA`, `AE` with one `ERR` per
  refusal, or `AR`, echoing MSH-10 in MSA-2.
- `write`: any message composed from segments, fields, repetitions,
  components and subcomponents in the message's delimiters, each value
  escaped by HL7 v2.9 chapter 2 §2.7.1 or echoed as read, checked against the
  message structure its MSH declares and encoded in the character set MSH-18
  names. A writer `with_profile` checks against the structure as an
  integration profile constrains it, the constraints carried as data in
  `parse::profile` (`IHE_ITI_21`: QRI optional in the ITI-21 response, IHE
  ITI TF-2 Table 3.21-6, and QPD-3 repeating in the query and its echo,
  §3.21.4.1.2.3). The acknowledgment and every reply's header are
  built on it.
- `inbound`: the steps above for one message, settling the `AR` and `AE`
  answers a message owes before it is mapped.
- `map`: the interpreter of the `hl7.fhir.uv.v2mappings` 1.0.0 ConceptMaps.
  The message, segment and data type maps are generated into the crate at
  build time from the pinned guide (`map::maps::Maps::guide`), with
  FerroBRIDGE's corrections in the guide's shape beside them
  (`Maps::corrected`, each one the run uses counted as `supplemented`). The
  message map names a resource per segment, the segment map maps the fields,
  a data type map maps the components, and every table value is translated by
  `ConceptMap/$translate` on a terminology server through `ferrobridge-term`'s
  client: load the guide's package and FerroBRIDGE's value package
  (packed from `terminology/hl7v2-values/` in the repository by
  `scripts/terminology/pack-hl7v2-values.sh`) into it. `map::single` maps
  one segment to one resource, a PID to a `Patient`. `map::inverse` writes a
  resource back as a segment through the same maps, a `Patient` as a PID,
  with every element it does not write back counted. The FHIR side is written
  through `fhirconnect::tree` over the `fhir-types` R4 element table, and the
  output is an R4 message `Bundle`. Every condition, target or value the run
  cannot carry is a typed, counted outcome.

## Features

- `map` (default): the v2-to-FHIR interpreter. It needs `fhir-types`,
  `fhirconnect` without its `mapping` feature and `ferrobridge-term` with its
  `client` feature, so it links no openEHR crate and no `jsonschema`.

With `map` off the crate is the listener, the decoder, the parser, the writer
and the acknowledgment:

```toml
ferrobridge-hl7v2 = { version = "0.6", default-features = false }
```

## Example

Receive one message from an MLLP frame and build its acknowledgment:

```rust
use ferrobridge_hl7v2::ack::{Code, Stamp};
use ferrobridge_hl7v2::decode::{Charset, DecodeError};
use ferrobridge_hl7v2::inbound::{self, Received};
use ferrobridge_hl7v2::parse::structure::structure_for;

/// The bytes to send back for one MLLP frame, or `None` when no header can be
/// read to address an answer to.
fn answer(frame: &[u8], stamp: Stamp<'_>) -> Result<Option<Vec<u8>>, DecodeError> {
    match inbound::receive(frame, Charset::Ascii, structure_for, stamp) {
        // Map and commit `inbound.parsed()` here; answer AE on a refusal.
        Received::Parsed(inbound) => inbound.answer(Code::Accept, &[], stamp).map(Some),
        // The message owed AR or AE before it could be mapped.
        Received::Answered { reply, .. } => Ok(Some(reply)),
        Received::Unanswerable => Ok(None),
    }
}
```

The `# Examples` sections of `inbound::receive`, `decode::decode`,
`ack::render` and `mllp::Codec` carry the tested forms.

## Versions

The crate is on the FerroBRIDGE crate line, which follows Cargo SemVer (see
Requiring the crate above). It requires its `hl7v2-types` sibling exactly, so
take `hl7v2-types` at the version this crate names.

## Licence

Business Source License 1.1 (`LICENSE`): free for every non-production use and
for non-commercial production use; a commercial licence for other production
use; Apache License 2.0 four years after each version.
