# Quickstart — Homelab Zombie

This walks you from zero to your first diagnostic conversation in about
10 minutes.

## Prerequisites

- A homelab with at least one of:
  - A Kubernetes cluster (k3s, k8s, microk8s)
  - One or more Docker hosts
  - SSH-accessible Linux machines
- A machine in your homelab network that can run Docker, to host the
  worker (1 CPU, 512Mi RAM is enough)
- A UseZombie account — sign up at https://usezombie.com

## Step 1. Install zombiectl

    brew install usezombie/tap/zombiectl

Or:

    curl -fsSL https://get.usezombie.com | sh

Verify:

    zombiectl version

## Step 2. Log in

    zombiectl auth login

Opens a browser window, you approve, CLI is now authenticated.

## Step 3. Install the homelab skills

    zombiectl install homelab

This registers the homelab skill bundle on your account. It includes:

- kubectl-readonly
- docker-readonly
- ssh-readonly
- the homelab reasoner

## Step 4. Start the worker in your homelab
<<I have tailscale running, so no the zombie worker is already connected to my tailscale in ovh>>
so we have to write instruction to that effect.
On a box in your homelab network, run:

    docker run -d --name zombie-worker \
      --restart unless-stopped \
      -e ZOMBIE_TOKEN=$(zombiectl worker token) \
      usezombie/worker:latest

The worker polls the UseZombie control plane. It holds your credentials
locally. The control plane never has direct network access to your
homelab.

Verify it's connected:

<<We cant do this? This is not generic>>

    zombiectl worker list

You should see your new worker as `online`.

## Step 5. Add a credential

<<Is this the right command?>>
For a Kubernetes cluster:

    zombiectl cred add kube \
      --name home-k3s \
      --file ~/.kube/config \
      --context default

The kubeconfig is encrypted and stored locally on the worker. The UseZombie
control plane never sees it.

For a Docker host over SSH:

    zombiectl cred add ssh \
      --name homelab-01 \
      --host homelab-01.tailnet \
      --user kishore \
      --key ~/.ssh/id_ed25519

## Step 6. Talk to your zombie

Start a session:

    zombie
    → Homelab zombie ready. What's up?

Describe the problem:

    > Jellyfin pods keep restarting

The zombie reasons in a loop — kubectl get, describe, logs, top — and
reports back. Ask follow-up questions in the same session:

    > what about immich, same issue?
    > now check if any node is under memory pressure

For one-shot use (scripting, aliases, cron later):

    zombie --once "check my kubernetes cluster for unhealthy pods"

To exit a session, Ctrl-D or `/exit`.

## Step 7. Review the audit log

    zombiectl log show <run_id>

Every tool call, every credential use, every decision.

## Next steps

- Try a specific question: `zombie` then `> Jellyfin is slow`
- Schedule nightly audits (coming in v0.2)
- Write your own skill: see Writing Skills
- Run the control plane yourself: see Self-Hosting
