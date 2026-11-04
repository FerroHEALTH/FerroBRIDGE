- `eehrxf`'s laboratory project carries a received result's reference range
  into the `normal_range` of its quantity and back out, and the order a
  report answers into the `KDS_Laborbericht` test request details by its
  identifier. A project now accounts for received elements
  (`Project::accounted`, `Mapping::accounting`): each received instance of
  one that the run carries into no structured element, such as a typed
  reference range or the order a result answers, is a `warning` issue in
  the composition's `OperationOutcome`. `ReceiveMappingError` gains
  `Original`. `fhirconnect` resolves a path below a polymorphic `ELEMENT`
  to the one value alternative whose class carries its tail, reads a bound
  below `normal_range` as the quantity's own class, and names the resource
  of each `Location` (`Location::resource`, `Type/id`).
