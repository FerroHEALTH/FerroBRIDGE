- **`ferrobridge-hl7v2` sends over MLLP** (#1281). `mllp::send` opens a
  connection within a connect timeout, writes one message as one MLLP frame
  and returns the one reply frame within a reply timeout, bounded by the
  codec's ceiling. A refused connection, either timeout, a peer that closes
  before or inside its reply, and a reply past the ceiling are each their own
  `SendError` variant, and none carries the message or the reply. FerroPIX
  sends its ITI-10 notifications through it.
