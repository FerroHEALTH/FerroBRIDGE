- The operator API exports a mapping-set revision as a ZIP at
  `GET /operator/revisions/{id}/export`, the same bytes at every export of
  the same files, and imports one at `POST /operator/revisions/import`: the
  ZIP sent raw as `application/zip` with a reason, read with a ceiling,
  checked with the provisioned files as a dry run and saved as a revision
  that does not run until an apply. An import that does not compile is
  refused with its diagnostics, and a body that is no mapping-set archive
  answers the new `bad-archive` problem. The operator console exports from a
  revision's page and the Mapping set's More menu, and imports from the new
  Import a mapping set page.
