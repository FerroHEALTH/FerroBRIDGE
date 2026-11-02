- The operator console has an Events page (`/events`): each change-feed
  consumer's source, reader and position, its health, the messages it
  published since start and its last failure with the cause, then the AMQP
  exchange, the routing-key grammar and the bound queues, read-only from the
  bridge's new `GET /operator/events`. A bridge with no events lane says so
  in one line.
