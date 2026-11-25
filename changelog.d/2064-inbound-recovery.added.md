- Redelivery and reconciliation per inbound source: a webhook source's
  `recovery` declares a `redeliver` call, made when a replica first leads
  the source and after a run of `503` answers, and since-cursor `reconcile`
  polls per tenant that mark every returned entity due again. One replica
  runs a source's recovery under its lane lease, and a cursor moves only
  under the lease generation that read it. `[inbound] recovery_tick` sets
  how often each replica looks for due recovery, and
  `POST /operator/sources/{id}/reconcile` and `…/redeliver` run the steps on
  demand.
