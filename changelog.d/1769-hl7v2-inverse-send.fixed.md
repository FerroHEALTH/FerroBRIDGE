- `ferrobridge-hl7v2`'s `map::inverse::segment` returns a `Send` future, so
  it can run inside a task on a multi-threaded tokio runtime.
