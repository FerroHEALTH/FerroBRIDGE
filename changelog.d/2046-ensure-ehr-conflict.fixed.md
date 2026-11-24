- When the CDR answers `409 Conflict` to an EHR creation because another
  writer created the EHR for the same subject first, the facade reads the EHR
  by subject again and writes into it, so concurrent first writes for one
  patient converge on one EHR in place of a refusal.
