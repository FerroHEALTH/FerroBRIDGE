- The `eehrxf` HDR project maps a procedure only at the commenced,
  completed and aborted careflow steps, through a FHIRconnect preprocessor
  gate, and receives a `Procedure` only with the matching status. A
  discharge report that holds a planned procedure, or one at any other
  step, now maps with that procedure declared skipped and its elements
  declared left out. Before, the run failed whole on the `Procedure` it
  wrote with no `status`.
