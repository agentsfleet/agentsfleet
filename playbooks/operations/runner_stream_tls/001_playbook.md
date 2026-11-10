---
type: how-to
audience: operator
verified: 2026-11-09
product_version: 0.75.0
executable: false
---

# Generate certificates for the private runner listener

## What this does

These certificates let runners verify the private `agentsfleetd` endpoint over HTTP/3 or HTTP/2.
Create separate material for development (`ZMB_CD_DEV`) and production (`ZMB_CD_PROD`).

Both transports require TLS 1.3 with `X25519MLKEM768` hybrid post-quantum key exchange.
Certificates use Elliptic Curve Digital Signature Algorithm (ECDSA), so server authentication is not post-quantum.
Handshake keys need no vault entry.

## Before you begin

Use OpenSSL 3 in a private shell with command tracing disabled.
If an environment already has a trusted issuer, export its certificate and signing key as `ca.crt` and `ca.key`.
Skip issuer generation for that environment.

## Steps

1. Create a protected working directory for development.
   Use `runner-stream-prod` instead when preparing production.

   ```bash
   set +x
   umask 077
   mkdir -m 700 runner-stream-dev
   cd runner-stream-dev
   ```

   Success prints nothing.

2. Generate an issuer only when that environment has none.

   ```bash
   openssl req -x509 -newkey ec -pkeyopt ec_paramgen_curve:P-256 \
     -noenc -sha256 -days 3650 -subj '/CN=agentsfleet runner CA' \
     -addext 'basicConstraints=critical,CA:TRUE,pathlen:0' \
     -addext 'keyUsage=critical,keyCertSign,cRLSign' \
     -keyout ca.key -out ca.crt
   ```

   OpenSSL writes `ca.key` and `ca.crt`; progress output varies.

3. Generate the server key and certificate request.

   ```bash
   openssl req -new -newkey ec -pkeyopt ec_paramgen_curve:P-256 \
     -noenc -sha256 -subj '/CN=agentsfleet runner listener' \
     -keyout server.key -out server.csr
   ```

   OpenSSL writes `server.key` and `server.csr`; progress output varies.

4. Write the certificate extensions for development.
   For production, use `DNS:agentsfleetd-prod.internal` in the command below.

   ```bash
   printf 'basicConstraints=critical,CA:FALSE\nkeyUsage=critical,digitalSignature\nextendedKeyUsage=serverAuth\nsubjectAltName=DNS:agentsfleetd-dev.internal\n' > server.ext
   ```

   Success prints nothing.

5. Sign the server certificate to expire with its issuer after 10 years.
   The certificate stops validating when its issuer expires, so renew both together.

   ```bash
   openssl x509 -req -sha256 -days 3650 -in server.csr \
     -CA ca.crt -CAkey ca.key -CAcreateserial \
     -extfile server.ext -out server.crt
   ```

   OpenSSL reports `Certificate request self-signature ok`.

6. Build the certificate chain.

   ```bash
   cat server.crt ca.crt > server-chain.pem
   ```

   Success prints nothing.

7. Save these file contents in one `agentsfleet-runner-stream` item in the matching vault: `ZMB_CD_DEV` or `ZMB_CD_PROD`.
   Keep private-key fields concealed.

   | Field | File or value |
   |---|---|
   | `ca-certificate` | `ca.crt` |
   | `ca-private-key` | `ca.key`; signing only, never delivered to runtime; concealed |
   | `certificate-chain` | `server-chain.pem` |
   | `private-key` | `server.key`; daemon only; concealed |
   | `server-name` | `agentsfleetd-dev.internal` or `agentsfleetd-prod.internal` |
   | `wireguard-config` | Existing DEV host configuration; concealed; DEV only |

   Runners receive only the public CA certificate and keep their existing bearer tokens.

## Verify it works

Check development's certificate before copying it to the vault.
For production, use `agentsfleetd-prod.internal` below.

```bash
openssl verify -CAfile ca.crt -verify_hostname agentsfleetd-dev.internal server.crt
```

Expected output: `server.crt: OK`.
The development and release workflows load the stored fields and deliver daemon certificate files and runner public trust.
The runner deployment gate checks a recent native heartbeat after restart.

DEV bootstrap also restores its private Fly tunnel and checks both native transports.

## Remove or undo

After checking the vault copies, delete the local working directory through your file manager.
Keep the issuer in 1Password for future certificate renewals.
Never paste private keys into chat or repository files.

## Related pages

- [Listener settings and trust](../../../docs/architecture/runner_protocol.md#certificates-and-configuration)
