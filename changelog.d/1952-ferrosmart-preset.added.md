- The bridge runs behind FerroSMART, the FerroHEALTH SMART App Launch 2.x
  authorization server: an `[[auth.issuer]]` takes `profile = "ferrosmart"`
  from `ferrohealth-smart` 0.29.1, which reads FerroSMART's RFC 9068 tokens
  with `typ` `at+jwt` alone and `scope` required, admits a person through a
  SMART app the table lists in `delegates` and a Backend Services client as
  a machine. The Operate section has a page for the deployment, with
  FerroSMART's resource table, the bridge's issuer table and the discovery
  document. The server and the operator console move to
  `ferrohealth-smart` 0.29.1 and `ferrohealth-session` 0.24, and two
  `[[auth.issuer]]` tables naming one issuer now refuse the start.
