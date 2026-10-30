- The operator console's server no longer panics on a disposed router value
  while it draws a page, which could fail a page with a `500`: the console
  draws every page in one pass with no router left to dispose (#1355).
