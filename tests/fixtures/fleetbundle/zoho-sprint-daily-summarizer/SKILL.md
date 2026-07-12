---
name: zoho-sprint-daily-summarizer
description: "Summarizes the day's Zoho Sprints activity and posts a digest."
version: 0.1.0
---
# Zoho Sprints daily summarizer

Reads the day's Zoho Sprints activity and posts a concise digest.

## Goal
Once a day, pull the sprint's recent item changes and produce a short digest of
what moved, what is blocked, and what is due.

## Steps
1. Read recent sprint item activity from the Zoho Sprints API with `http_request`.
2. Group by status change, blockers, and due-today.
3. Produce a concise markdown digest.

## Constraints
- Read-only against Zoho Sprints; do not mutate items.
- Stay within the declared Zoho network hosts.
