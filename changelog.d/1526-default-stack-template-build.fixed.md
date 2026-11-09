- A debug build of `eehrxf` builds an operational template from the CKM
  archetypes, compiles the EU MPD and EPS projects and maps them on a
  thread with the default 2 MiB stack, so a consumer no longer needs a
  larger stack for it. The overflow was in the `openehr-lang` lexer, which
  0.0.130, the pinned version, fixes (FerroHEALTH/FerroEHR#4961).
