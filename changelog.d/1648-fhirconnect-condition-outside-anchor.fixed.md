- A FHIRconnect `openehrCondition` whose target lies outside the repeating
  node its mapping reads (a `targetRoot` under `$archetype/protocol`, say,
  on a mapping inside an event) reads the target's own instance coming out
  of openEHR instead of the input's instance at the same depth, so the
  condition no longer fails for every event past the first.
