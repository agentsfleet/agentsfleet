# I built an AI agent that diagnoses my homelab without holding my kubeconfig

I run a small homelab. Three machines, a k3s cluster, Jellyfin, Immich,
Paperless, a bunch of other self-hosted things I half-remember installing.
Every few weeks something breaks at the worst possible time, usually because
a disk filled up or a container ran out of memory while I was asleep. I
end up SSH-ing into the right machine at 11pm, pasting logs into ChatGPT,
copy-pasting `kubectl` output back and forth until I figure out what broke.

ChatGPT is helpful in that loop, but it's not *enough*. It can't see my
logs. It can't `kubectl top pod`. It gives me the next command to run,
I run it, I paste the output back. It takes forever.

The obvious answer: give an agent my kubeconfig and let it run `kubectl`
itself. Which I am, frankly, not going to do. My kubeconfig has
cluster-admin. A hallucinated `kubectl delete ns media` at 11pm while I'm
tired and not reading carefully is not a risk I'm willing to take. Same
reason I don't give Claude Code my production AWS credentials.

So I built the thing I actually wanted: an AI agent that can investigate
my homelab, with real `kubectl` access, but **can't break anything**, and
never holds the credential in its own context.

It's called Homelab Zombie. It's the first sample built on UseZombie v2.
It's open source. Here's how it works and what I learned building it.

## What it does

You talk to it:

    $ zombie
    → Homelab zombie ready. What's up?

    > Jellyfin pods keep restarting

The zombie reasons in a loop:

    [00:02] kubectl get pods -A | grep jellyfin
            → 2 pods found, both in CrashLoopBackOff
    [00:05] kubectl describe pod jellyfin-0 -n media
            → Last state: OOMKilled, exit code 137
    [00:07] kubectl logs jellyfin-0 -n media --previous --tail=200
    [00:10] kubectl top pod jellyfin-0 -n media
            → memory: 512Mi (at limit)
    [00:12] kubectl get deployment jellyfin -n media -o yaml
            → resources.limits.memory: 512Mi
    [00:14] kubectl describe node homelab-02
            → MemoryPressure: False, 8Gi available

And at the end, produces a report:

    Root cause (high confidence): Jellyfin is being OOM-killed. Memory limit
    is 512Mi; usage spikes above this during library scans and transcoding.

    Proposed fix:
        kubectl patch deployment jellyfin -n media -p '{...}'

    Risk: low. Node has 8Gi free.

Then you can keep going in the same session:

    > does immich have the same problem?
    > now check if any node is under memory pressure

The zombie maintains context across turns. It's a conversation, not a
command tree.

And in v0.1, it stops at the proposal. No writes. No restarts. It's a
diagnostic agent, not a remediation agent. That's a feature, not a
limitation — I want to trust the thing before I hand it a hammer.

## Three things that make this safe to run

### 1. The agent's tools are allowlisted at the verb level

The agent has a skill called `kubectl-readonly`. It's a shim around kubectl
that only permits `get`, `describe`, `logs`, `top`, `events`. Every command
the agent wants to run is parsed, checked against the allowlist, and
executed if and only if it passes. If the agent asks to run
`kubectl delete`, the skill returns an error. The agent reasons with that
error and tries something else.

Crucially, `secrets` is denied even for `get`. Log exfiltration via secrets
is the first thing a bad agent run would try. It can't.

### 2. The agent never holds the kubeconfig

This is the part I am most proud of.

In most agent-plus-kubectl setups, the agent is a process that has
`KUBECONFIG` set in its environment. That means the agent's LLM context,
in theory, can be prompt-injected into echoing the bearer token. People
have done this to Claude Code and to various self-hosted agents. It's a
real attack surface.

In UseZombie, the agent process literally does not have the credential.
What it has is a placeholder string — a random UUID that looks nothing
like a token. When `kubectl` in the sandbox makes an HTTPS call to the
cluster API, a proxy at the network boundary catches it, swaps the
placeholder for the real credential held in the vault, and re-originates
the request. The real token never enters the memory of the process the
LLM is driving.

If you're skeptical this is meaningfully different from "short-lived token
in env" — it is, because short-lived tokens still appear in prompt-injection
exfiltration paths. Placeholders don't. The model can repeat the
placeholder all day; it does nothing on its own.

(This same pattern is how Microsandbox works, and credit where it's due —
they shipped it before we did. UseZombie extends it with audit + approval
layers + the skill registry, and targets homelab / ops use cases rather
than generic code execution.)

### 3. The worker runs in my network, not in the cloud

The UseZombie control plane runs on our infrastructure and coordinates
runs, stores audit logs, etc. But the worker that actually executes tool
calls runs on a box *inside* my homelab. For me, it's a small Docker
container on my k3s control plane node. It polls the control plane for
tasks, executes them against my cluster, streams reasoning back.

The control plane never has a route to my k3s API. My kubeconfig never
leaves my network. I can see every command the worker ran, down to the
exact kubectl invocation. If I pull the plug on the worker's container,
the whole thing stops.

## What went into building this

UseZombie v2 is a framework for agents that touch real infrastructure.
Homelab Zombie is one sample built on it. The framework provides:

- **A worker binary** you run in your network (Docker, or a binary on
  a Pi), which polls the control plane and executes tool calls
- **A vault** for credentials that never leave the worker's boundary
- **A skill registry** — each skill is a markdown file declaring its
  policy, allowed verbs, and audit shape
- **A reasoning loop** (the zombie) that selects skills, plans, invokes,
  observes, and iterates
- **A kill switch** — `zombiectl kill <run_id>` stops everything in-flight
- **An audit log** that captures every tool call, every credential use,
  every decision

If you want to build your own zombie — one that manages your side projects
repo, one that watches your self-hosted services, one that does something
I haven't thought of — you write a 40-line markdown skill spec and a
prompt. The framework does the rest.

## What's next

v0.1 of Homelab Zombie is read-only. v0.2 adds writes behind approval
gates: the agent can propose a `kubectl patch`, push the proposal to my
phone via Slack, and wait for a tap. That's when this stops being a
diagnostic tool and starts being an actual SRE for the 3am page.

If you run a homelab and want to try it:

- Install: `brew install usezombie/tap/zombiectl`
  (or `curl -fsSL get.usezombie.com | sh`)
- Skill install: `zombiectl install homelab`
- Docs: https://docs.usezombie.com/quickstart/homelab
- GitHub: https://github.com/usezombie/usezombie

It's Apache-2.0. The worker is self-hostable. If you'd rather not depend
on our cloud control plane, you can run the entire thing on your own
infrastructure — that path is documented too.

Feedback and issues welcome. I'm still looking for design partners —
r/selfhosted people, Jellyfin-at-home-breaks-at-3am people, anyone running
k3s in their closet. If that's you, open an issue or DM me.

— Kishore
