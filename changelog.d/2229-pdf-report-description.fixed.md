- `eehrxf` 0.41 writes the `DocumentReference` of a document exchanged as an
  embedded PDF (draft EEHRxF Art 3(1)(a)) without claiming HL7 Europe
  `EehrxfMhdDocumentReference`, which holds `attachment.data` to 0..0. The
  embedded PDF/A stays, as the act asks, and the Annex III `status` is
  written as `docStatus` again. Every description writes its
  `masterIdentifier.type` as IHE MHD `uniqueId`, which
  `IHE.MHD.UniqueIdIdentifier` 4.2.5 requires.
