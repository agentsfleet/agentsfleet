---
type: how-to
audience: operator
verified: 2026-11-09
product_version: 0.75.0
executable: false
---

# Set up the private runner listener

## Status

Setup draft for M243's morning handoff; runtime integration and final deployment verification remain pending.
Keep [#819](https://github.com/agentsfleet/agentsfleet/pull/819) unmerged until its checks and the setup handoff are complete.
The [runner protocol](../../../docs/architecture/runner_protocol.md) owns the transport and trust rules.

## Reuse existing credentials

Use the existing environment vault selected by `VAULT_DEV` or `VAULT_PROD`.
Reuse the deployment token and each runner's existing bearer token.
Runners do not need client certificates.

Reuse an existing private issuer when its certificate and signing key are available.
Reuse a leaf certificate only when it covers the configured private hostname and remains valid.
The public Fly edge's TLS configuration does not configure this separate private listener.

The development example below uses `agentsfleetd-dev.internal`.
Production uses its own issuer and `agentsfleetd-prod.internal`.
Keep issuer signing keys outside daemon and runner runtime settings.

## Prepare protected local files

Use OpenSSL 3 and a signed-in 1Password command-line interface.
Run these commands in a private operator shell with command tracing disabled.
They create local material and do not change Fly or 1Password.

```bash
set +x
set -euo pipefail
umask 077
M243_TLS_DIR="$(mktemp -d "${TMPDIR:-/tmp}/agentsfleet-m243-tls.XXXXXX")"
M243_SERVER_NAME="agentsfleetd-dev.internal"
M243_VAULT="$(gh variable get VAULT_DEV --repo agentsfleet/agentsfleet)"
```

### Reuse an existing issuer

Choose this block when `runner-stream-ca` already exists in the selected vault.

```bash
op read "op://$M243_VAULT/runner-stream-ca/certificate" > "$M243_TLS_DIR/ca.crt"
op read "op://$M243_VAULT/runner-stream-ca/private-key" > "$M243_TLS_DIR/ca.key"
```

### Create an issuer when none exists

Use this block only for an environment without an existing runner-stream issuer.
Its output replaces neither an existing issuer nor a runner token.

```bash
openssl req -x509 -newkey ec -pkeyopt ec_paramgen_curve:P-256 \
  -noenc -sha256 -days 3650 \
  -subj '/CN=agentsfleet development runner CA' \
  -addext 'basicConstraints=critical,CA:TRUE,pathlen:0' \
  -addext 'keyUsage=critical,keyCertSign,cRLSign' \
  -keyout "$M243_TLS_DIR/ca.key" -out "$M243_TLS_DIR/ca.crt"
```

## Issue and verify the server certificate

Skip issuance when an existing valid certificate and its matching key meet the hostname requirement.
Otherwise issue a 90-day leaf certificate from the selected issuer.

```bash
openssl req -new -newkey ec -pkeyopt ec_paramgen_curve:P-256 \
  -noenc -sha256 -subj "/CN=$M243_SERVER_NAME" \
  -keyout "$M243_TLS_DIR/server.key" -out "$M243_TLS_DIR/server.csr"
printf 'basicConstraints=critical,CA:FALSE\nkeyUsage=critical,digitalSignature\nextendedKeyUsage=serverAuth\nsubjectAltName=DNS:%s\n' \
  "$M243_SERVER_NAME" > "$M243_TLS_DIR/server.ext"
openssl x509 -req -sha256 -days 90 \
  -in "$M243_TLS_DIR/server.csr" \
  -CA "$M243_TLS_DIR/ca.crt" -CAkey "$M243_TLS_DIR/ca.key" -CAcreateserial \
  -extfile "$M243_TLS_DIR/server.ext" -out "$M243_TLS_DIR/server.crt"
cat "$M243_TLS_DIR/server.crt" "$M243_TLS_DIR/ca.crt" > "$M243_TLS_DIR/server-chain.pem"
openssl verify -CAfile "$M243_TLS_DIR/ca.crt" \
  -verify_hostname "$M243_SERVER_NAME" "$M243_TLS_DIR/server.crt"
```

The verification must report `OK` for the server certificate.
The daemon separately verifies that the private key matches its certificate before serving.

## Store the material in 1Password

Fill these fields in the existing environment vault using the 1Password application.
Keep private keys concealed and copy file contents directly into their fields.
Do not paste keys into chat, repository files or terminal output.

| Item | Field | Contents | Delivered to runtime |
|---|---|---|---|
| `runner-stream-ca` | `certificate` | `ca.crt` | Public runner trust bundle |
| `runner-stream-ca` | `private-key` | `ca.key`, concealed | Never |
| `agentsfleetd-runner-stream-tls` | `certificate-chain` | `server-chain.pem` | Daemon |
| `agentsfleetd-runner-stream-tls` | `private-key` | `server.key`, concealed | Daemon |
| `agentsfleetd-runner-stream-tls` | `server-name` | Configured private hostname | Configuration metadata |

The existing development runner item is `agentsfleet-dev-runner-ant`.
Its `runner-token` remains unchanged.
Production runner items come from the existing `PROD_RUNNER_HOSTS` inventory.

## Runtime delivery and rollout

The final M243 report must supply the tested file paths, environment settings and commands for staging these values from 1Password.
It must identify any Fly configuration edits requiring Indy's approval before applying them.
No deployment or vault update is claimed by this draft.

Deploy the issuer's public certificate to runner trust before enabling a leaf signed by that issuer.
An external runner also needs a network route to the private endpoint.
It must permit outbound QUIC over UDP or the HTTP/2 fallback over TCP.

Leaf rotation uses a daemon rollout and the runner's ordinary reconnect path.
Keep overlapping public trust anchors during issuer rotation.
Remove the protected temporary directory only after the vault copies and runtime delivery are verified.

## Post-quantum scope

The planned TLS 1.3 preference is X25519 plus ML-KEM-768 hybrid key exchange.
Its ephemeral handshake keys do not require additional 1Password entries.
The ECDSA certificate signatures above use conventional cryptography; they do not provide post-quantum server authentication.
