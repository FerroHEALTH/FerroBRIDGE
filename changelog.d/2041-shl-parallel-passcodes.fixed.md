- Parallel passcode guesses against one SMART Health Link check the
  passcode against its Argon2id hash at once, and only the attempt count
  waits on the link's write, so a burst of guesses no longer runs past the
  request timeout. The lifetime attempts are still counted exactly.
