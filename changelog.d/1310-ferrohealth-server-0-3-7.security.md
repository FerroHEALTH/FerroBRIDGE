- The server and the operator console take `ferrohealth-server` 0.3.7 in
  place of 0.3.6 (GHSA-2mc2-h3jq-c5wg). Every earlier release held a
  plain-HTTP connection that never sent a byte for as long as the client
  kept it open, so a client with no credentials could exhaust the process's
  file descriptors, and the listener then retried a failed `accept` at once
  and logged every failure. Such a connection now closes once
  `[server] header_read_timeout_ms` passes, counted from accept, and the
  listener backs off after an `EMFILE` or `ENFILE`. A response to a request
  whose body the routes left unread, such as a `404` to a `POST` of an
  unserved path, now carries `Connection: close`.
