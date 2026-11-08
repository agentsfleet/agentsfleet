---
type: how-to
audience: operator
verified: 2026-11-08
product_version: 0.75.0
executable: false
---

# Register the SMTP relay

## What this does

Stores the Simple Mail Transfer Protocol (SMTP) relay as the `smtp-relay` bag, so `agentsfleetd` sends one email per invite.
Resend is the relay today; any relay with username and password login works by changing the bag.
Without the bag, each invite records `email_status: "unconfigured"` and the owner copies the link by hand.

Indy owns the relay account, domain records and 1Password; the agent syncs and verifies.
Both environments send from `agentsfleet.net`.

## Before you begin

The environment's [admin bootstrap](../admin_bootstrap/001_playbook.md) is complete and `/readyz` passes.

## Steps

1. Indy adds `agentsfleet.net` in [Resend domains](https://resend.com/domains).
2. Indy publishes every Domain Name System (DNS) record it lists: DomainKeys Identified Mail (DKIM), Sender Policy Framework (SPF) and the bounce `MX` record.
3. Indy waits until the domain shows **Verified**.
   An unverified domain refuses every send, and each invite records `email_status: "failed"`.
4. Indy creates one API key per environment with **sending access** only, limited to `agentsfleet.net`.
5. Indy stores the `smtp-relay` fields, per [Send with SMTP](https://resend.com/docs/send-with-smtp):
   - `host`: `smtp.resend.com`
   - `port`: `465`, implicit Transport Layer Security (TLS); any other port must offer STARTTLS, which the daemon requires
   - `username`: `resend`
   - `password`: the API key
   - `from_address`: an address on the verified domain, such as `hello@agentsfleet.net`
6. The agent syncs `smtp-relay` with the [shared bag procedure](../admin_bootstrap/001_playbook.md#sync-a-provider-secret-bag).
   The daemon reads the bag on each send, so a new password applies to the next invite without a restart.

## Verify it works

1. Indy invites a test mailbox from the members page.
2. The invite dialog reads **Invite sent**.
3. The message arrives from `from_address` with the subject `You're invited to join <ACCOUNT> on agentsfleet`, where `<ACCOUNT>` is the inviting account's name.
4. Indy opens the link, signed in as that mailbox's account, and accepts.
5. The agent records the relay's message ID and the accepted invite in the Pull Request Session Notes.
