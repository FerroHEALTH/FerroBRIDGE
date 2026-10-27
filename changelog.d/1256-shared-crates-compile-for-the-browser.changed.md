- `openehr-mapping-core` and `ferrobridge-operator-api` compile for
  `wasm32-unknown-unknown`, so the operator console's browser half can draw
  the bridge's documents. The workspace takes `openehr-its` without its
  ITS-REST client, which only the server adds, and the console CI lane
  checks the operator contract for wasm32
  ([#1256](https://github.com/FerroHEALTH/FerroBRIDGE/issues/1256)).
