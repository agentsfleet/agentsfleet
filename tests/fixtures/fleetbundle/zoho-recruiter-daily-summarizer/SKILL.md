---
name: zoho-recruiter-daily-summarizer
description: "Summarizes the day's Zoho Recruit pipeline activity and posts a digest."
version: 0.1.0
---
# Zoho Recruit daily summarizer

Reads the day's Zoho Recruit pipeline movement and posts a concise digest.

## Goal
Once a day, pull the candidates whose stage changed and produce a short digest of
who advanced, who is waiting on us, and which roles are going cold.

## Steps
1. Read recent candidate and job-opening activity from the Zoho Recruit API with `http_request`.
2. Group by advanced, waiting-on-us, waiting-on-them, and rejected.
3. Flag any open role with no candidate movement for seven days.
4. Produce a concise markdown digest.

## Constraints
- Read-only against Zoho Recruit; do not advance, reject, email, or edit.
- Stay within the declared Zoho network hosts.
- Name candidates and their stage only. Never quote resume contents, salary
  expectations, contact details, or interviewer notes.
