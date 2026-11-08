- The `openehr-*` crates move to 0.0.133. `openehr_mapping_core::template::TemplateSource::Opt2`
  gains `prohibited_attributes`, the attributes the template removed for
  `existence matches {0}` as `openehr_adl::opt::create_opt` reports them in
  its `RawOpt`, which the Web Template leaves out; a consumer that builds an
  `Opt2` source passes them, or an empty list for an OPT2 received without
  them, as the bridge does for an OPT2 the CDR serves.
