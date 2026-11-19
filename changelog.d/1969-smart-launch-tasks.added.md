- The FHIR facade serves SMART launch `Task`s (SMART App Launch 2.2.0, Task
  profile for requesting a SMART launch) behind `[facade.launch_task]`:
  create, read and search by `patient`, `status` and `code` on `Task`, every
  `Task` checked against `task-ehr-launch` or `task-standalone-launch` and a
  `Task` of neither refused, kept in a store of its own; the
  `CapabilityStatement` declares both profiles.
