- A pull request records its user-visible effect as a fragment,
  `changelog.d/<issue>-<kebab-slug>.<section>.md` (`changelog.d/README.md`),
  and no longer edits `CHANGELOG.md`, so two open pull requests stop
  conflicting on `[Unreleased]`. The release cut assembles the fragments into
  the version's section with `scripts/release/changelog.sh --assemble` and
  deletes them, and the release `plan` job refuses a tag that still has one.
  CI refuses a malformed fragment, a change that writes under `[Unreleased]`,
  and a change to a user-visible surface with no fragment unless it carries
  the `no-changelog` label. `scripts/gh/changelog-merge.sh` is retired.
