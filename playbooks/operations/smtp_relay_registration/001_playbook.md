# Register the SMTP Relay

**Owners:** 🤠 Indy for the relay account, domain records and 1Password; 🦉 Orly
for secret sync and verification
**Updated:** Oct 01, 2026
**Prerequisite:** the target environment's admin bootstrap is complete and its
API host passes `/readyz`

`agentsfleetd` sends one email per invite through the Simple Mail Transfer
Protocol (SMTP) relay named by the `smtp-relay` platform bag. Resend is that
relay today; any relay that speaks SMTP with a username and password plugs in
by changing the bag. Without the bag, invites still work: each one records
`email_status: "unconfigured"` and the owner copies the link by hand.

| Environment | Vault | API base | Sending domain |
|---|---|---|---|
| Development | `ZMB_CD_DEV` | `https://api-dev.agentsfleet.net` | `agentsfleet.net` |
| Production | `ZMB_CD_PROD` | `https://api.agentsfleet.net` | `agentsfleet.net` |

## 1. Indy: verify the sending domain

In the relay's dashboard ([Resend domains](https://resend.com/domains)), add
`agentsfleet.net` and publish every Domain Name System (DNS) record it lists:
the DomainKeys Identified Mail (DKIM) key, the Sender Policy Framework (SPF)
record, and the bounce `MX` record. Do not continue until the domain shows
**Verified**; an unverified domain refuses every send, and each invite then
records `email_status: "failed"`.

## 2. Indy: create the SMTP credential

Create one API key per environment with **sending access** only, restricted to
`agentsfleet.net`. Resend's SMTP settings are documented in
[Send with SMTP](https://resend.com/docs/send-with-smtp):

- host `smtp.resend.com`;
- port `465` (implicit Transport Layer Security (TLS)); any other port must
  offer STARTTLS, which the daemon requires;
- username `resend`;
- password: the API key.

## 3. Indy: vault the five fields

In the matching 1Password vault, create or update `smtp-relay` with:

- `host`.
- `port`.
- `username`.
- `password`.
- `from_address` — an address on the verified domain, for example
  `hello@agentsfleet.net`.

Use the 1Password application. Never paste a value into chat, a ticket, or a
shell command.

## 4. Orly: sync the platform bag

After Indy approves the target, run:

```bash
ENV=dev \
ALLOW_VAULT_READS=1 \
ALLOW_PLATFORM_SECRET_WRITES=1 \
  ./playbooks/lib/platform_secret_sync.sh smtp-relay
```

Change `ENV` to `prod` only for the production run. The daemon reads the bag
per send, so a rotated password takes effect on the next invite without a
restart.

## 5. Indy and Orly: prove the live path

1. Indy invites a test mailbox from the members page.
2. The row reads **Email sent**, and the message arrives from `from_address`
   with the subject "You're invited to join {account} on agentsfleet".
3. Indy opens the link while signed in as that mailbox's account and accepts.
4. Orly records the relay's message ID and the accepted invite in the Pull
   Request's Session Notes.

## Complete when

- The sending domain is verified.
- The five-field bag exists in the admin workspace.
- A real invite email arrives and its link accepts.
