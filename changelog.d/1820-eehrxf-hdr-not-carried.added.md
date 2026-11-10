- `eehrxf`: mapping a received discharge report with the `HDR` project
  declares in its `OperationOutcome` each section none of whose entries
  the project carries, such as the hospital course, and each element of a
  carried entry that the composition does not hold, such as
  `Condition.severity` and `verificationStatus`. Each is an `incomplete`
  warning at the element's FHIRPath. The composition's original content
  still keeps every one of them.
