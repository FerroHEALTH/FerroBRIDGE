- The operator console answers `GET /session` with the idle and absolute
  seconds its session has left, never counting as activity, `POST /session`
  extends the idle timeout, and both answer `401` once the session is gone,
  with an RFC 9457 problem whose `detail` is in the console's language.
  Every signed-in page carries FerroUI's session watch, which warns before
  the idle timeout with Stay signed in, and ends every tab of the console
  together when the session ends or the operator signs out, leading to the
  sign-in with the page left as its return address.
