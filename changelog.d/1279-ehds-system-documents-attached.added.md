- **Each release attaches the EHR system's information sheet and
  instructions for use.** The EHR system Cadasto B.V. declares under the
  EHDS is FerroEHR with FerroBRIDGE, and both documents are pages of the
  FerroHEALTH book. The release lane takes them at the FerroHEALTH commit
  `docs/ehds/system-documents.env` pins, refuses a page whose revision
  differs from the pin, and attaches `ferrobridge-<tag>-information-sheet.md`
  and `ferrobridge-<tag>-instructions-for-use.md` with the release's facts
  appended: the system version, the release date (EHDS Art. 38(2)(b)) and
  the versions of the standards FerroBRIDGE supports (Art. 38(2)(e)). The
  release notes name the pinned revision of each system page.
