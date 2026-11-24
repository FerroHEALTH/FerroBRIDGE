- The patient summary carries problems, allergies and intolerances, and
  procedures: FerroBRIDGE's patient summary template adds a problem list
  (`EVALUATION.problem_diagnosis.v1`), an allergies and intolerances section
  (`EVALUATION.adverse_reaction_risk.v2`) and a history of procedures
  (`ACTION.procedure.v1`), and `eehrxf` 0.24.0's EPS project maps them to
  `condition-eu-core`, `allergyIntolerance-eu-core` and `procedure-eu-eps`
  with their SNOMED CT codes. `Patient/$summary` fills those sections from
  the record, and a fed section the record leaves empty is `nilknown`.
