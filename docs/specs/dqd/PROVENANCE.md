<!-- This file describes vendored third-party material; the bytes beside it
     keep their upstream licence, not the licence of this repository. -->

# Provenance: the OHDSI Data Quality Dashboard CDM v5.4 check definitions

Vendored verbatim by `scripts/vendor/dqd.sh`
(.claude/rules/vendored-inputs.md). Never edit a file here: change the pin in
docs/VERSIONS.md and re-run the script.

- Source: <https://github.com/OHDSI/DataQualityDashboard>
- Pin: tag `v2.9.0`, which resolves to commit `b9d2a9b4a4736f7ea81415d974c72119b6f0bd11`
- Fetched: 2026-10-08
- Upstream licence: `Apache License 2.0`. **The repository has no `LICENSE` file.**
  `DESCRIPTION` is the only place the licence is declared, which is why that
  file is vendored here and the script fails if a `LICENSE` file ever appears.
- Layout: the upstream paths, unchanged
- Files: 46
- Tree digest (sha256 over the sorted per-file `sha256  path` listing,
  `PROVENANCE.md` excluded): `17e0fca8deae7bb353fb3e7e5ea7e39e94dcf0dd41ffac9e77a6a35bac8c7155`

## What is here

The Rust port of the Dashboard's conformance and completeness checks reads
these files as its oracle (Blacketer et al., JAMIA 2021,
doi:10.1093/jamia/ocab132). The four CSV files are the check catalogue and the
CDM v5.4 thresholds that enable and parameterise each check. The SQL templates
are the fifteen in-scope checks in the SQL Server dialect the Dashboard
renders through SqlRender; the bridge's PostgreSQL form is its own
translation. The R sources and the vignettes carry what no CSV or SQL file
states: the threshold evaluation, the not-applicable rules, the check id, and
the result and overview shapes.

The thresholds and the enablement come from these files, never from the
legacy check columns of the CDM's own `OMOP_CDMv5.4_Table_Level.csv`.

| File | sha256 | git blob id |
|---|---|---|
| `DESCRIPTION` | `c0a395880eb5ba3bb0983a847903e14f83c2af9dc1bdee179d55b00aacca5b9e` | `ccb64a262322d86db45afdff1f14d0ca408d566a` |
| `inst/csv/OMOP_CDMv5.4_Check_Descriptions.csv` | `882047d00095bd4b67ffc68a53987bc38b8dfc551aae4479efd61d85fe409ee1` | `087ba8e91192c5cdba43f3c2459d430762c1bf88` |
| `inst/csv/OMOP_CDMv5.4_Table_Level.csv` | `daa5ef1cf72abd59f921edc668623c8ae884b0f2af0a8f46fd7fc019db7a0f09` | `042de9f112c9bfa3e61a69224e72e7854ae7e276` |
| `inst/csv/OMOP_CDMv5.4_Field_Level.csv` | `812bd8f48f6bd9cf0bdde35ef9dca3ef147c86cdcff5283a5d7c94e10c9d1d9a` | `115c1fcda100ad2051a4dce9e7ecc7989cef8cf1` |
| `inst/csv/OMOP_CDMv5.4_Concept_Level.csv` | `cd663cd1c631505f4609cec93ba44c4932291f1bf2fcb259f866f52549ec9185` | `e216718d9f5d035439a42f12cf712a55b06eae58` |
| `inst/sql/sql_server/table_cdm_table.sql` | `ce1e15c64c826aa96539c1399cca61c3a93d7f39b373a30c0c13984115ac37e7` | `14e2fc973eff1ad782c7fa6b791be9e3808ee483` |
| `inst/sql/sql_server/table_person_completeness.sql` | `8d58144aad182e1fc0dc8cb264d1148c971292d89ff41b84bb6710509eaff964` | `c43482354aaea11c0ecfbfe37cd156449b4fb7dd` |
| `inst/sql/sql_server/table_condition_era_completeness.sql` | `47a9015312ce1eda761be591b38322995f6f52cda1e5d39f1f4b4917048f2fa7` | `0b5265e8dcfd34ad85a7f64a87a0f646fd73e76f` |
| `inst/sql/sql_server/field_cdm_field.sql` | `862fe259dcb95cdebf1108f27be31c8b0c34e3390850b9e3a4e0c3f49eca89cc` | `c7746c5bcd9c943c9a9a3b4d7b9e83d1908a17b3` |
| `inst/sql/sql_server/field_is_not_nullable.sql` | `1d9c137975214fcb9298be24b6de6c05f1a7bf0476b7ec16fe3a4e6f7ef94111` | `6808e3cf02be347cff16b523a1fd68a8e801593b` |
| `inst/sql/sql_server/field_cdm_datatype.sql` | `5599da5a97295c622ad0d469e48d1c1937a1cb89679bf9855e2e60279aac5926` | `fb7a329adabdb1a8b2c5a9bd801bb4e538541323` |
| `inst/sql/sql_server/field_is_primary_key.sql` | `a92f980da8df6a922ca04b3722d5ab908157416d15a7cadd28cd940a2a391662` | `e524a80effe9da856104d22f94c2ae6c247eb0d6` |
| `inst/sql/sql_server/is_foreign_key.sql` | `09b6040117177c0c9f935e9d35b5995ee4746a4b2f610c5e7f1433978e766863` | `71ba4c93c192ace1b1930b303216d027569cc9a7` |
| `inst/sql/sql_server/field_fk_domain.sql` | `37f3284e113f0c26c9d3e945604a865b8e7d80cabe0ef0479846080dfe10808d` | `dd8cc17451fea57c4e229e62e44bf32c83a63b85` |
| `inst/sql/sql_server/field_fk_class.sql` | `53d3e1b57f2b9ea5f4575e39e61702e9a2f7cd56986a384bba65cb69fd815efd` | `04fbc1fcc06ac44ee8b9386c1e004cdb365aeec7` |
| `inst/sql/sql_server/field_is_standard_valid_concept.sql` | `36a5d8e62661beb5f73e2b6417b349b397d5b8ac934715fb94c887749f8eb3c6` | `1130e2f5e6ab2bbb5096db430850a7977effbd38` |
| `inst/sql/sql_server/field_measure_value_completeness.sql` | `c4adbe23c6977b1c14225161646a4999c8bfd46616b7aec497b79464121edb5d` | `410b4492bc867e56018501ed502314efb43c79e7` |
| `inst/sql/sql_server/field_concept_record_completeness.sql` | `0c25537d081ff28854a145ac4cc17b875222cadc9dc0d37c31829437e181e342` | `c11a6b405c6b9de894a7654b3dc7f518bbe5407e` |
| `inst/sql/sql_server/field_source_value_completeness.sql` | `e5367662577872ca6462790c5185b4e29d9cb2a1dfc58c36a6f889af6f4c562b` | `04b223d1127c505459facbffd9e0ab182b013cca` |
| `inst/sql/sql_server/result_dataframe_ddl.sql` | `595c41d47b81a64130c3d52f5fcf8776c891e93f42effff3f5cd78cd2521dbcd` | `13ac645f7acdc5af59aa98b0a66c177c0f994e44` |
| `R/evaluateThresholds.R` | `1cb28d906370f6bfd12acab8b96c0c854572e3dfb59d4e4aaad8005aca41a840` | `e866eb9dbdc10c5db0910c9b786f842bf9e95592` |
| `R/calculateNotApplicableStatus.R` | `237956091867fa4b3898916e8d87cc4f27c08aeca169dfe9abb4b52c749ea78f` | `e47ffe6a427de722df4f71cf259bacbd9195b61b` |
| `R/recordResult.R` | `0b0d80af5892550faac8a7f9a083355c1dd825ed4954647dcb3668098487edae` | `e3e5f569eb0fa57b23d1bde3bb2b971a77581083` |
| `R/getCheckId.R` | `c30988fbac11d997cef168be4e961ffcc42141bbf5e0a9fd199646d1f7b51e76` | `5e703e26a17c57fdca94d6148cf901010429efaa` |
| `R/summarizeResults.R` | `a627959ba41646a83d680f852a8c51a2c583ec6c3ca9c21cd6d466a37fc7e3b6` | `34b875d58124f3c09c5dc03d8b002332b1a70a9b` |
| `R/executeDqChecks.R` | `dd543d62f75c363060f17b784f3bc45c07d63a7c222be1cb24a56fc586c30271` | `c91db772efecdf2f0849f0420432a6c467a07253` |
| `R/runCheck.R` | `a33c6ad96e91753813fdc463bb815105aac11ce0ce767db89aac5acc0004b0a0` | `a94b7c1513d122b0e73f959cce59811600106531` |
| `R/processCheck.R` | `c39808530641b7e10cbea69839f287d7f8bf05d8e60fc839b990b4a5f5829c28` | `cdc39a43e2cb8a7511ae6573c474b23501638b73` |
| `R/writeResultsTo.R` | `96927b4b5d3f7e6430a52a604375b5b9a6da23fc423275aaf4f4a7a73f3cf622` | `d2aea8cda61646ef6f2360b9db8ff776af501105` |
| `vignettes/CheckStatusDefinitions.rmd` | `54992c184e9d9bba1d325253038934e26c5fa9806538dea5e01d474346777e1f` | `357a2c20ebfb2e38302fdda212c951311be231c8` |
| `vignettes/Thresholds.rmd` | `e63d1dd3d2b63d83809c04d8576302e57ed0b47e9ecef7611f7f6a75a726d25d` | `d162ba16b4de91d016f9e30e9637eedc0838953d` |
| `vignettes/checks/cdmTable.Rmd` | `8eaaf9f76c717668243cfb07a1108dc06c99a5fe2a871a7916f9948b70603a8a` | `c1ed076d8fe589a31af40b8b4ae71a58bdba5572` |
| `vignettes/checks/cdmField.Rmd` | `68cf8b556538c891499572c14ec4759b7054587871a020e82bda7550a478d919` | `105cb2b863592a76c6b94d77d85dec77edfb7cba` |
| `vignettes/checks/isRequired.Rmd` | `74b6aeb8df89c57bc474475bc672b29dfde1a4a9c3ede390cc349063a3cdb5da` | `3beec0c2aea32fa49d38a4249bf7e32f6172981a` |
| `vignettes/checks/cdmDatatype.Rmd` | `c54990ae62c106ff96cd0a4375c4d7e4c4b9f7b14df817019dcfeb41c2f3fc2a` | `facb31ce0bd45a8eb3f8d4a61250d91735b19d97` |
| `vignettes/checks/isPrimaryKey.Rmd` | `568a502d90181ef4b8709351dd5ea7155b1bd0e05e444745efea87dbfb2a5531` | `f7a7686d232a6239de7a3a120cc53564bdc773cd` |
| `vignettes/checks/isForeignKey.Rmd` | `9e1194cd3e6a8459f9d41aeea20d667d91786f2fc280e41a4eaf1a06ddc62e20` | `12e527f1b822365e7365148a212c1f23a379e3e3` |
| `vignettes/checks/fkDomain.Rmd` | `ea5382028ff6625595638f6dd716e83a6f539dca1f308487abc7831f1daacb0a` | `6cf295049c0f536551ffe32faa25b8d4729f9f4d` |
| `vignettes/checks/fkClass.Rmd` | `fb6eaea9406bb944194995f5cd23e5f93616c1bd9e5bc76a607eac7c9a87183c` | `dd7192134530cf72f1f83394e912e47d91994b28` |
| `vignettes/checks/isStandardValidConcept.Rmd` | `370ca78dcb77e1ab9be7ad3ef59a06596a8322ec00174c4a71fc996ae7f56fa7` | `eccb8864d6b67ea56958174c95f07016abad9afb` |
| `vignettes/checks/measurePersonCompleteness.Rmd` | `2a7a90dafe0c2a098d86b7a34e34ea28c37920bfe2ab0383be17982a3c93277d` | `7e1531c91a63d5b58172c5b1de06ef22bfcc6b60` |
| `vignettes/checks/measureConditionEraCompleteness.Rmd` | `4f2b2f27a4decaf86990202b6eb76ad803b862e5886ab7f256e148b303b87d7a` | `f19fae6345355ad821e547f224bce26b01d0233e` |
| `vignettes/checks/measureValueCompleteness.Rmd` | `2a58046f0483c4e71b7b85c499b3beb1e447672343c7528b7fbbce7d0469940c` | `9cd61f76219aa015b5531359aafebfbfa081b560` |
| `vignettes/checks/standardConceptRecordCompleteness.Rmd` | `ce811c54e0a96e103c63d152d6593d73cd01064acfcbe1fb05df621e90c6d22e` | `782b26ccd7269f63291804a204d3936625af0d87` |
| `vignettes/checks/sourceConceptRecordCompleteness.Rmd` | `c9f4d3aa0891a70a1a6ef86ce32c580ffb91eb613b02b061477d3a79f92c36bf` | `9b3c9ced94487599bde0e183b74b92160573f294` |
| `vignettes/checks/sourceValueCompleteness.Rmd` | `4228ec15f446116d188a23e68ea44c49f204a956e3d4aa9a1647880080e70b19` | `6001516c3f31392fcd82f5a68123c5c921098f27` |
