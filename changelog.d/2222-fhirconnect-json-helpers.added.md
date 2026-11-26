- `fhirconnect` exports `tree::json_kind`, which names a JSON value's kind
  without its content, and `operations::contract::parameters::body_object`
  and `parameters_from_object`, the two steps `parameters_from_json` reads a
  request body in.
