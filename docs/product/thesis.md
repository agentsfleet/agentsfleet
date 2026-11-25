---
type: explanation
audience: contributor
verified: 2026-11-25
product_version: 0.90.0
executable: false
---

# Product thesis

## What it is

`agentsfleet` runs AI agents, called fleets, that keep working after a person stops typing.
A fleet wakes on an event, such as a pull request, a schedule or a webhook.
It gathers evidence from real systems, reports what it found and asks a person before risky work.

## Who it is for

It is for engineering teams who own production systems and their recurring chores.
They already ask a terminal assistant for help, but that help ends when the session ends.

## Why it exists

Operational work falls between tools: CI (continuous integration), logs, dashboards, chat and shell history.
The person on call stitches the evidence together and remembers what was tried.
A fleet owns that outcome instead: it keeps the history, resumes after failure, and continues until done or blocked.

Three choices shape the product:

- **Open source.** An operator can read the code that holds their credentials.
- **Bring your own model key.** A team may pay its model provider directly; [models](../architecture/models.md#where-the-provider-key-lives) owns the details.
- **Chat-authored.** A person's first message creates the fleet; later messages and pinned memory steer it.

## What it deliberately does not do

- It is not a general personal assistant, and it does not promise to automate anything.
- It is not a chat window over tools; a fleet must move an outcome forward, not only summarize logs.
- It grants nothing from prose: a fleet acts only with access a person approved.

If opening a local assistant and asking "what should I do next?" gives the same value, the product has failed.
The [fixture fleets](../../tests/fixtures/fleets/README.md) show the jobs it does today.
