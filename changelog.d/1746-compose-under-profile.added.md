- `ferrobridge-hl7v2` 0.1.204 composes a v2 message against its structure
  as an integration profile constrains it: `Writer::with_profile` takes a
  `parse::profile::Profile`, whose constraints set the minimum cardinality
  of named structure nodes, and the crate ships `IHE_ITI_21`, which makes
  the QRI of the ITI-21 RSP^K22 optional (IHE ITI TF-2 §3.21.4.2.2, Table
  3.21-6) where the v2.5 `RSP_K21` requires it. Breaking:
  `parse::grouping::group` takes the profile as a third argument; pass
  `&Profile::BASE` for the base structure.
