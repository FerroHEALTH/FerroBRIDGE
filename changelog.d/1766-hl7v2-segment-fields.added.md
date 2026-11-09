- `ferrobridge-hl7v2`'s `write::value::Segment` returns its fields
  (`fields`, `field(position)`) and sets one (`set(position, field)`), so a
  consumer can fill a field no map writes back, such as PID-1 or a PID-3
  from its own identifier domains, in a segment `map::inverse::segment`
  returns.
