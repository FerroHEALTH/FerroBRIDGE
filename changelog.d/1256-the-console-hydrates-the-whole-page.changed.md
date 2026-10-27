- The operator console hydrates the whole page on FerroUI 0.5.0: the
  server draws the first page and the browser draws every page after it,
  so a link, a tab, the search or an action no longer reloads the document,
  and the browser's back and forward walk the pages you visited. The pages
  load in five groups, each the first time you open one of its pages or
  earlier while the browser is idle. Every page and action still works with
  no WebAssembly, as plain links and form posts. The facade's
  `CapabilityStatement` reaches the browser reduced to the new
  `ferrobridge-operator-api` `capability::Capability` document, which the
  console's server writes, and the entry WebAssembly's budget rises to
  646 KiB
  ([#1256](https://github.com/FerroHEALTH/FerroBRIDGE/issues/1256)).
