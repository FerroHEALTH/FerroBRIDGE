- A FHIRconnect `hierarchy.split` whose node lies outside the repeating
  anchor of the file it is slotted from no longer takes that anchor's
  instance for the repeating nodes above it: each group creates the next
  free element under the instances those nodes are bound to. Coming out of
  openEHR, a mapping whose node sits below two or more repeating nodes its
  parent does not bind reads every instance of each, where it read only the
  first instance of all but the outermost.
