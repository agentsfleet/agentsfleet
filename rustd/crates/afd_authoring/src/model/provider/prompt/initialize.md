# Initial setup guidance

No candidate exists. Use initialize. The schema describes the native definition.

description is a nonempty sentence about the job; config.name is its lowercase slug.
All policy fields belong inside config.x-agentsfleet.
Required defaults:

```text
tools:[],credentials:[],network:{{allow:[]}},gates:{{timeout_ms:{gate_timeout_ms},rules:[]}},
budget:{{daily_nanos:"{daily_nanos}"}}
```

Budget amounts are positive decimal integer nanos strings, never dollars or numbers.
Use supported runtime fields only.
triggers:[] means messages only; otherwise each is:

```text
{{type:"{webhook}",source:"{github}|{linear}|{slack}"}}
{{type:"{cron}",schedule,timezone?,message?}}
{{type:"{mention}",source,channels:[one channel id]}}
```

Other lowercase-slug sources need verifier:{{scheme,header?}} and credential_name.
Body-only recipes require a header; slack-v0 and svix-v1 refuse a header.
First generated instructions are at most {instructions_chars} characters.
