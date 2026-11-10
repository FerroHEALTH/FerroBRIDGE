- A `manual` entry that writes free text (a `value` and no `defining_code`)
  at a `DV_CODED_TEXT` node whose value set is open writes a `DV_TEXT`
  there, which FLAT carries under `|other` (Simplified Formats, Open
  Value-Sets), and an `openehrCondition` reading that node with an empty tail
  compares its text; the run refused the entry with a decode error before.
  At a node whose value set is closed the entry is refused at load with the
  new `fc-closed-value-set`. `fhirconnect` moves to 0.2: a consumer that
  matches `ResolveCode` exhaustively adds an arm for
  `ResolveCode::ClosedValueSet`; `eehrxf` moves to 0.5 and
  `ferrobridge-hl7v2` to 0.7 with it. `ResolvedNode::list_open` in
  `openehr-mapping-core` reads the Web Template's `listOpen`.
