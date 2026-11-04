- `fhirconnect`'s `$tofhir` no longer maps a resource from the composition
  root when the composition holds no occurrence of the context's start
  archetype. That resource held only a context's constant mappings, for a
  record that is not there. The run now refuses as not applicable, as it does
  when the context's condition admits no occurrence (FHIRconnect 1.0.0
  `context-mappings.adoc` §Start, `rest-api.adoc` §$tofhir). `eehrxf`'s
  patient summary medication context drops the guard it carried against it.
