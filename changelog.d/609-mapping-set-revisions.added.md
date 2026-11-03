- With `[operator] mapping_dir`, the bridge keeps each state of the console's
  mapping files it ran as a revision under `mapping_dir/..revisions`: at
  start-up, after each save, after a reload that finds the files changed and
  after each apply, with who, when and why. The operator API lists them at
  `GET /operator/revisions`, answers one at `GET /operator/revisions/{id}`
  and its line diff against the running one at
  `GET /operator/revisions/{id}/diff`, and applies one at
  `POST /operator/revisions/{id}/apply` under `If-Match` on the running
  revision with a reason, compiling the set first and changing nothing when
  it does not compile. Provisioned files are in no revision. The operator
  console lists the revisions on the Mapping set's Revisions tab and shows
  each one's diff with Roll back to this revision.
