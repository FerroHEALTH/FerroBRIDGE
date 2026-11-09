- The null flavours, the default
  `EVENT_CONTEXT.setting`, the version container of an `OBJECT_VERSION_ID`,
  the `ETag` entity tag, the ISO 8601 parts of a `DV_DURATION`, a `DV_TIME`
  and a visit's start and end, and the interface form of an archetype id are
  read through those crates. Breaking for a consumer of `fhirconnect`:
  `engine::lens::null_flavour::FLAVOURS` gives way to `fhir_default`, and
  `BRIDGE` pairs each code with an `openehr_term::codes::NullFlavour`;
  `engine::lens::duration::Component` gives way to
  `openehr_base`'s `DurationDesignator`; and
  `engine::identity::versioned_object_uid` returns a `String`.
