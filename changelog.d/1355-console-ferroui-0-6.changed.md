- The operator console moves to FerroUI 0.6.0 and draws every page on its
  server: a link, a search, a tab or an action loads one complete page, and
  FerroUI's small script modules add the editor's highlighting, the search's
  suggestions, the menus and the dialogs on top, so every page and every
  action works with them refused. The console carries no WebAssembly, its
  Content-Security-Policy is `script-src 'self'; style-src 'self'` with no
  nonce, and its stylesheet, theme and modules are served by the binary
  under `/ferrohealth-ui/0.6.0/`. Reload the set, the editor and the
  playground post to their own page, which draws the bridge's answer beside
  what you entered.
