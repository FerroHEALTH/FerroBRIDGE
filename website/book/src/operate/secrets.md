<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# Named secrets

An inbound source's document names its secrets and never holds their values:
`{"secret": "nedap-ons-connector-secret"}`. Each name is a file in the
`[secrets]` directory ([`[secrets]`](configuration.md#secrets)), so a value
never sits in a state store row, a log line or an operator API answer. No
specification governs how a bridge reads its source secrets: this is
FerroBRIDGE's own design.

## How a name resolves

- A file's name is the secret's name, and its content is the value, a
  trailing newline removed. A client certificate's secret holds the PEM
  certificate and key.
- The directory is read at start and again every `poll_seconds`, so a
  secret you add or rotate reaches every replica without a restart.
- A source write that names a secret the directory does not hold is refused
  with `422`.
- A secret missing at runtime fails its source closed: the intake answers
  `503`, and a work item that needs the secret runs again later.

## Rotating a key

A verifier takes a list of keys and accepts any of them, newest first. To
rotate, add the new secret to the directory, add its name to the source's
`keys` before the old one, switch the sender to the new key, and then remove
the old name and its file. A client certificate rotates by replacing its
file: the client is built again when the content changes.

## Kubernetes

Mount one Secret as the directory, with no `subPath`. The kubelet updates a
Secret mounted as a volume in place, while a `subPath` mount never updates
(<https://kubernetes.io/docs/concepts/configuration/secret/#mounted-secrets-are-updated-automatically>).

```yaml
volumes:
  - name: source-secrets
    secret:
      secretName: ferrobridge-sources
containers:
  - name: ferrobridge
    env:
      - name: FERROBRIDGE__SECRETS__DIR
        value: /run/secrets/ferrobridge-sources
    volumeMounts:
      - name: source-secrets
        mountPath: /run/secrets/ferrobridge-sources
        readOnly: true
```

## Compose and plain files

Write the new value to a file beside the old one and rename it over it, so a
read never sees a half-written file.
