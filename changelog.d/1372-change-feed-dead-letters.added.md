- Every change-feed consumer follows the one `[change_feed]` source on its
  own task under its own reader name, with its own health indicator. A
  poisoned event is retried up to `[change_feed] max_attempts` passes (5 by
  default) and then parked with its cause in the dead-letter store
  `[change_feed] dead_letters` names, which is required; a transient failure
  never counts towards parking. `GET /operator/events` lists every consumer
  with its parked events, `POST /operator/events/{consumer}/parked/{id}/replay`
  offers one again before the consumer's next page, and the console's Events
  page lists them with a Replay button. The `dead-letters-unavailable`
  problem type answers a store that refuses.
