- The operator console moves to `ferrohealth-session` 0.32, which removes the
  sealed session store: `[session] store = "sealed"` and the `key` and
  `key_file` keys are now unknown and refused when the configuration loads.
  Sessions stay in the console's memory, so run one console instance, or
  route each session to one replica at the ingress (sticky sessions).
