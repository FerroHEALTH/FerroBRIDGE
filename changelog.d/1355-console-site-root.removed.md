- The operator console's `[server] site_root` key
  (`FERROBRIDGE_CONSOLE__SERVER__SITE_ROOT`) is gone, and the console image
  carries no `/site` directory: the binary serves everything the pages load.
  A configuration that still sets the key is refused at load, naming it;
  remove the line.
- The release no longer publishes the console's site bundle
  (`ferrobridge-console-<tag>-site.tar.gz` and its attestations).
