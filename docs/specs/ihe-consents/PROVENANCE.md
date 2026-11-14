<!-- This file describes vendored third-party material; the bytes beside it
     keep their upstream licence, not the licence of this repository. -->

# Provenance: IHE Basic and Advanced Patient Privacy Consents

Vendored by `scripts/vendor/consent.sh`, each artefact pinned by its
URL and the sha256 of its bytes. Never edit a file here: change the pins in
the script and the pin-set digest in docs/VERSIONS.md, and re-run the script.

- Pin-set digest (sha256 over the sorted `mode  file  url  sha256` lines of
  the pins): `2ff22a756ac982860f6a0a35765492455e942bef3c183d66d7fe21eac1e2fd1d`
- Fetched: 2026-11-14, with the User-Agent `ferrobridge-vendor (scripts/vendor)`
- Artefacts: 4 committed, 0 cache only, 0 needing
  manual retrieval
- Files in this directory: 4 besides this one, each verbatim as the
  publisher serves it
- Tree digest (sha256 over the sorted per-file `sha256  path` listing,
  `PROVENANCE.md` excluded): `6dfe7122e5156c93349d289be5382e206fa960fa9872c6de9ad1f5964f0a978c`
- Read by: FerroHEALTH/FerroBRIDGE#1933 (app/ferrobridge-server/src/consent and its tests)

IHE Basic Patient Privacy Consents (BPPC) as the HTML IT Infrastructure
Technical Framework publishes it, and the Advanced Patient Privacy Consents
(APPC) Trial Implementation supplement, each file whole without the
stylesheets, scripts and images the pages link. The HTML Technical Framework
is republished in place at each revision, so a re-run fails once IHE
publishes a new one; move the pins then. The General Introduction chapter on
copyright licences is taken as the grant the copy rests on.

## Artefacts

### `ch-19.html` (committed)

- Source: <https://profiles.ihe.net/ITI/TF/Volume1/ch-19.html>
- Version: ITI TF-1 chapter 19, Volume Revision 20.0, August 4, 2023, Final Text
- sha256: `a35ecb229ef67302051e92b13f3cd59ee12e67bd9d463b7166f720b6b055455f`
- Licence: IHE Technical Frameworks General Introduction, chapter 9 (vendored here as ch-9.html): "an irrevocable, worldwide, perpetual, royalty-free, nontransferable, nonexclusive, non-sublicensable license under its copyrights in any IHE profiles and Technical Framework documents ... to reproduce and distribute" them
- Note: BPPC, the profile: policy identifiers, actors, options, process flow

### `ch-5.1.html` (committed)

- Source: <https://profiles.ihe.net/ITI/TF/Volume3/ch-5.1.html>
- Version: ITI TF-3 section 5.1, Volume Revision 20.0, August 4, 2023, Final Text
- sha256: `82d220535db84971ed0162a8674e384938d7ae944a82cb217200391933c5b7b8`
- Licence: IHE Technical Frameworks General Introduction, chapter 9 (vendored here as ch-9.html): "an irrevocable, worldwide, perpetual, royalty-free, nontransferable, nonexclusive, non-sublicensable license under its copyrights in any IHE profiles and Technical Framework documents ... to reproduce and distribute" them
- Note: BPPC, the Patient Privacy Consent Acknowledgment Document content module

### `ch-9.html` (committed)

- Source: <https://profiles.ihe.net/GeneralIntro/ch-9.html>
- Version: IHE Technical Frameworks General Introduction, chapter 9
- sha256: `2056d5b878f1bee7f3881cada792ec352476613b495afc677b9a467b9d2197cf`
- Licence: IHE Technical Frameworks General Introduction, chapter 9 (vendored here as ch-9.html): "an irrevocable, worldwide, perpetual, royalty-free, nontransferable, nonexclusive, non-sublicensable license under its copyrights in any IHE profiles and Technical Framework documents ... to reproduce and distribute" them
- Note: Copyright Licenses, the grant this copy rests on

### `IHE_ITI_Suppl_APPC.pdf` (committed)

- Source: <https://www.ihe.net/uploadedFiles/Documents/ITI/IHE_ITI_Suppl_APPC.pdf>
- Version: APPC supplement Rev. 1.4, Trial Implementation, July 2, 2021
- sha256: `823bd5e015ddfb384a19f80ed93e9d1946740393a4be057fd8c9ab6b01936282`
- Licence: IHE Technical Frameworks General Introduction, chapter 9 (vendored here as ch-9.html): "an irrevocable, worldwide, perpetual, royalty-free, nontransferable, nonexclusive, non-sublicensable license under its copyrights in any IHE profiles and Technical Framework documents ... to reproduce and distribute" them
- Note: APPC, ITI TF-1 chapter 43 and ITI TF-3 section 5.6, the Privacy Consent Document content module
