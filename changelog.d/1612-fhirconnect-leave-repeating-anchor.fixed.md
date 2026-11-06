- A FHIRconnect mapping below a `followedBy` anchored at a repeating node can
  name a node outside that anchor, by `../` or by an absolute `$archetype`
  path, and inside a `reference` placed there: the FLAT build no longer
  refuses it with an occurrence count mismatch, and the node keeps the
  instance of only the repeating nodes it lies below.
