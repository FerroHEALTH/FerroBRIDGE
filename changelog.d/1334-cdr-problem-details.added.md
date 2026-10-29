- **The CDR client reads RFC 9457 problem details.** A CDR that answers its
  refusals as `application/problem+json` keeps its explanation on the facade:
  the problem's `title`, `detail`, each `errors` violation as `path: detail`,
  and its `type` reach the `OperationOutcome` diagnostics, as a
  `{message, validationErrors}` body still does. The composition and
  contribution writes already send `Prefer: return=representation`, so a CDR
  that lists violations only for such a write lists them.
