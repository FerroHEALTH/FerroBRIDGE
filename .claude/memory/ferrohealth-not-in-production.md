---
name: ferrohealth-not-in-production
description: No FerroHEALTH product is in production (owner, 2026-11-19), so design fixes are done as full refactors and rewrites, correct from the start, with no compatibility layer
metadata:
  type: project
---

No one runs FerroEHR, FerroSMART, FerroBRIDGE or the other FerroHEALTH products in production yet (owner, 2026-11-19). When a design turns out wrong or awkward, it is refactored or rewritten outright, correct from the start: no compatibility layer, no migration path for old behaviour, breaking changes taken with the version bump.

**Why:** the owner said "major refactor and change and rewrite because nobody is using it in production, so we can do it correct from the start", after the FerroSMART friction review (consent page for an organisation's own apps, fhirUser only from the IdP, patient tokens read as a professional's, three-place app registration, config-file-only admin, one upstream IdP, no grace on refresh rotation, front-channel logout missing).

**How to apply:** brief workers that they may replace a model, schema or config shape wholesale; still no test weakening and the specs stay the oracle. Same day the owner approved the #6060 relationship-based access control design (international core plus jurisdiction profiles) and reversed #619 so an upstream provider may use a client secret per provider. Related: [[no-compat-shims]], [[no-half-solutions]], [[fable-design-review-after-rewrite]].
