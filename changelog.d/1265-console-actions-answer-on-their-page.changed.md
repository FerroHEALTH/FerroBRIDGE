- **The operator console takes FerroUI 0.5.5** (`=0.5.5`), and Reload the
  set, the mapping editor's check and save, and the playground's runs post
  to their server functions through `<ActionForm>` in FerroUI's answer mode.
  With no WebAssembly the browser posts to `/api/reload`, `/api/edit` or
  `/api/run` and the console draws the page the form was on with the answer
  and what the operator entered; the browser's address stays at the
  function's. The console's own post routes for these actions
  (`POST /mappings`, `POST /mappings/new`, `POST /mappings/edit/…`,
  `POST /playground` and its tabs) are gone.
