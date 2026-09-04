"""Self-tests for ensure_fly_app.sh.

The script's whole reason for existing is that it REFUSES to report success
when an app is not actually running, so the negative cases are the point. A
fake flyctl is injected through $FLYCTL; nothing reaches Fly.
"""

import os
import subprocess
import tempfile
import textwrap
import unittest
from pathlib import Path

SCRIPT = Path(__file__).resolve().parent / "ensure_fly_app.sh"

def _machine(mid, state, checks):
    """One machine as `flyctl machine list --json` renders it.

    `checks` is a list of check statuses; Fly reports `passing`, `warning` or
    `critical`. An empty list is a real shape — a machine with no health check
    declared — and the script treats it as unproven rather than ready.
    """
    body = ",".join('{"name":"health","status":"%s"}' % c for c in checks)
    return '{"id":"%s","state":"%s","checks":[%s]}' % (mid, state, body)


def _machines(*specs):
    return "[" + ",".join(_machine(*s) for s in specs) + "]"


# Two machines started AND health-passing, which satisfies a desired count of
# 1 or 2. Both halves matter: `started` alone is the state Fly reports before
# the collector inside binds 4318.
STARTED_TWO = _machines(("a", "started", ["passing"]), ("b", "started", ["passing"]))
STOPPED_TWO = _machines(("a", "stopped", []), ("b", "stopped", []))
# Running, but the collector inside never came up — the race the health check
# exists to catch, and the one `state == "started"` reads as success.
STARTED_UNHEALTHY_TWO = _machines(
    ("a", "started", ["critical"]), ("b", "started", ["critical"])
)
# Running with no check declared at all: readiness is unprovable, not proven.
STARTED_UNCHECKED_TWO = _machines(("a", "started", []), ("b", "started", []))
NO_MACHINES = "[]"


class EnsureFlyAppTest(unittest.TestCase):
    def run_script(self, *args, machine_list, record=None,
                   app_exists=True, create_ok=True, fly_org="agentsfleet-dev"):
        """Run the script with a fake flyctl that prints `machine_list`.

        `app_exists` drives what `flyctl status` returns, which is how the
        script decides whether to create. `create_ok` drives `flyctl apps
        create`. Both default to the pre-existing behaviour, so every test
        written before create-if-absent keeps asserting what it always did.
        """
        with tempfile.TemporaryDirectory() as tmp:
            fake = Path(tmp) / "flyctl"
            log = Path(tmp) / "calls.log"
            fake.write_text(
                textwrap.dedent(
                    f"""\
                    #!/usr/bin/env bash
                    echo "$@" >> {log}
                    case "$1" in
                      "machine")  printf '%s' '{machine_list}' ;;
                      "image")    printf '%s' '[{{"Digest":"sha256:deadbeef"}}]' ;;
                      "status")   exit {0 if app_exists else 1} ;;
                      "apps")     exit {0 if create_ok else 1} ;;
                      *)          : ;;
                    esac
                    """
                )
            )
            fake.chmod(0o755)
            env = dict(os.environ, FLYCTL=str(fake), PATH=os.environ["PATH"],
                       POLL_ATTEMPTS="2", POLL_SLEEP_SECONDS="0",
                       FLY_ORG=fly_org)
            proc = subprocess.run(
                ["bash", str(SCRIPT), *args],
                capture_output=True,
                text=True,
                env=env,
            )
            calls = log.read_text() if log.exists() else ""
            if record is not None:
                record.append(calls)
            return proc, calls

    def test_running_app_at_desired_count_succeeds(self):
        proc, _ = self.run_script("otelcol-dev", "deploy/fly/otelcol-dev", "1",
                                  machine_list=STARTED_TWO)
        self.assertEqual(proc.returncode, 0, proc.stderr)

    def test_records_the_deployed_digest(self):
        proc, _ = self.run_script("otelcol-dev", "deploy/fly/otelcol-dev", "1",
                                  machine_list=STARTED_TWO)
        self.assertIn("sha256:deadbeef", proc.stdout)

    def test_deploys_from_context_when_no_machines_exist(self):
        # The positional build context is what makes the image's COPY resolve;
        # a deploy without it is the failure this asserts against.
        _, calls = self.run_script("otelcol-dev", "deploy/fly/otelcol-dev", "1",
                                   machine_list=NO_MACHINES)
        self.assertIn("deploy deploy/fly/otelcol-dev --app otelcol-dev", calls)

    def test_deploys_even_when_the_app_already_has_machines(self):
        # The regression this pins: deploying only when the app was empty left
        # config.yml — baked into the image by the Dockerfile's COPY — frozen at
        # whatever shipped first. Every later change to the receiver, the
        # authentication policy or the exporter pipeline was built and never
        # applied, which makes "the backend is a configuration change" false.
        _, calls = self.run_script("otelcol-dev", "deploy/fly/otelcol-dev", "1",
                                   machine_list=STARTED_TWO)
        self.assertIn("deploy deploy/fly/otelcol-dev --app otelcol-dev", calls)

    def test_deploy_precedes_the_scale_it_sizes(self):
        # Ordering, not just presence: scaling a release that the deploy is
        # about to replace sizes the wrong image.
        _, calls = self.run_script("otelcol-dev", "deploy/fly/otelcol-dev", "1",
                                   machine_list=STARTED_TWO)
        self.assertLess(calls.index("deploy "), calls.index("scale count "))

    def test_scales_to_the_desired_count(self):
        _, calls = self.run_script("otelcol-prod", "deploy/fly/otelcol-prod", "2",
                                   machine_list=STARTED_TWO)
        self.assertIn("scale count 2 --app otelcol-prod", calls)

    def test_fails_when_machines_never_start(self):
        # The defect the script exists to prevent: reporting success while the
        # collector is not serving, so the caller deploys a daemon at it.
        proc, _ = self.run_script("otelcol-dev", "deploy/fly/otelcol-dev", "1",
                                  machine_list=STOPPED_TWO)
        self.assertEqual(proc.returncode, 1)
        self.assertIn("never reached", proc.stderr)

    def test_fails_when_running_count_is_below_desired(self):
        one_started = '[{"id":"a","state":"started"},{"id":"b","state":"stopped"}]'
        proc, _ = self.run_script("otelcol-prod", "deploy/fly/otelcol-prod", "2",
                                  machine_list=one_started)
        self.assertEqual(proc.returncode, 1)

    def test_fails_when_machines_run_but_never_pass_health_checks(self):
        # Fly reports `started` when the VM is up, which precedes the collector
        # binding 4318. Gating on state alone would report success here and the
        # caller would point a daemon at a receiver that is not listening.
        proc, _ = self.run_script("otelcol-dev", "deploy/fly/otelcol-dev", "1",
                                  machine_list=STARTED_UNHEALTHY_TWO)
        self.assertEqual(proc.returncode, 1)
        self.assertIn("passed health checks", proc.stderr)

    def test_fails_when_a_machine_declares_no_health_check(self):
        # Unprovable is not the same as ready. If fly.toml loses [checks.health]
        # this must refuse rather than silently return to state-only readiness.
        proc, _ = self.run_script("otelcol-dev", "deploy/fly/otelcol-dev", "1",
                                  machine_list=STARTED_UNCHECKED_TWO)
        self.assertEqual(proc.returncode, 1)

    def test_the_two_refusals_name_different_causes(self):
        # "Not running" and "running but never healthy" are different incidents
        # with different first moves. A single message for both would make the
        # deploy log say less than it knows.
        stopped, _ = self.run_script("otelcol-dev", "deploy/fly/otelcol-dev", "1",
                                     machine_list=STOPPED_TWO)
        unhealthy, _ = self.run_script("otelcol-dev", "deploy/fly/otelcol-dev", "1",
                                       machine_list=STARTED_UNHEALTHY_TWO)
        self.assertIn("never reached", stopped.stderr)
        self.assertNotIn("never reached", unhealthy.stderr)
        self.assertNotEqual(stopped.stderr, unhealthy.stderr)

    def test_rejects_a_non_numeric_count(self):
        proc, _ = self.run_script("otelcol-dev", "deploy/fly/otelcol-dev", "two",
                                  machine_list=STARTED_TWO)
        self.assertEqual(proc.returncode, 2)

    def test_rejects_a_zero_count(self):
        # Zero would make every later assertion vacuous: nothing running still
        # satisfies "at least zero running".
        proc, _ = self.run_script("otelcol-dev", "deploy/fly/otelcol-dev", "0",
                                  machine_list=NO_MACHINES)
        self.assertEqual(proc.returncode, 2)

    def test_rejects_wrong_argument_count(self):
        proc, _ = self.run_script("otelcol-dev", machine_list=STARTED_TWO)
        self.assertEqual(proc.returncode, 2)


class CreateIfAbsentTest(unittest.TestCase):
    """The gap that took the development deploy down.

    Two collector apps shipped with a fly.toml, a Dockerfile, a config.yml and
    a deploy step in each of two workflows, and nothing ever created them. The
    priming playbook creates the apps it knew about; this script deployed,
    scaled and polled apps it assumed existed. So the first command to address
    one was `flyctl secrets set --app`, which fails on an app that is not
    there — and no gate was comparing the workflows against the playbook.
    """

    run_script = EnsureFlyAppTest.run_script

    def test_an_absent_app_is_created_before_the_deploy(self):
        _, calls = self.run_script("otelcol-dev", "deploy/fly/otelcol-dev", "1",
                                   machine_list=STARTED_TWO, app_exists=False)
        self.assertIn("apps create otelcol-dev --org", calls)
        self.assertLess(calls.index("apps create "), calls.index("deploy "))

    def test_an_existing_app_is_not_recreated(self):
        # Creating an app that exists is an error on Fly's side, so a script
        # that always created would fail every run after the first.
        _, calls = self.run_script("otelcol-dev", "deploy/fly/otelcol-dev", "1",
                                   machine_list=STARTED_TWO, app_exists=True)
        self.assertNotIn("apps create", calls)

    def test_a_failed_creation_refuses_rather_than_falling_through(self):
        # Falling through would deploy into nothing and then poll an app that
        # cannot answer, reporting the wrong cause twelve attempts later.
        proc, calls = self.run_script("otelcol-dev", "deploy/fly/otelcol-dev", "1",
                                      machine_list=STARTED_TWO,
                                      app_exists=False, create_ok=False)
        self.assertEqual(proc.returncode, 1)
        self.assertIn("could not create", proc.stderr)
        self.assertNotIn("deploy ", calls)

    def test_create_only_creates_without_deploying(self):
        # The ordering constraint: a fresh app must exist before `secrets set`
        # addresses it, and must not deploy until after — a collector booting
        # without its upstream credentials fails its health check, and this
        # script would then refuse for a reason that is nobody's bug.
        proc, calls = self.run_script("--create-only", "otelcol-dev",
                                      machine_list=NO_MACHINES, app_exists=False)
        self.assertEqual(proc.returncode, 0, proc.stderr)
        self.assertIn("apps create otelcol-dev --org", calls)
        self.assertNotIn("deploy ", calls)
        self.assertNotIn("scale count", calls)

    def test_create_only_is_idempotent(self):
        proc, calls = self.run_script("--create-only", "otelcol-dev",
                                      machine_list=NO_MACHINES, app_exists=True)
        self.assertEqual(proc.returncode, 0, proc.stderr)
        self.assertNotIn("apps create", calls)

    def test_create_only_rejects_a_wrong_argument_count(self):
        proc, _ = self.run_script("--create-only", machine_list=NO_MACHINES)
        self.assertEqual(proc.returncode, 2)

    def test_an_absent_app_with_no_org_refuses_rather_than_guessing(self):
        # Development and production are separate Fly organisations, so there
        # is no default that is right for both. Creating a production app in
        # the development org is not an error Fly reports — it is one somebody
        # finds later, which is why this refuses instead of picking.
        proc, calls = self.run_script("otelcol-prod", "deploy/fly/otelcol-prod", "1",
                                      machine_list=STARTED_TWO,
                                      app_exists=False, fly_org="")
        self.assertEqual(proc.returncode, 1)
        self.assertIn("FLY_ORG is unset", proc.stderr)
        self.assertNotIn("apps create", calls)

    def test_an_existing_app_deploys_without_an_org(self):
        # Only the create path needs FLY_ORG. Demanding it up front would break
        # every caller that never creates anything.
        proc, _ = self.run_script("otelcol-dev", "deploy/fly/otelcol-dev", "1",
                                  machine_list=STARTED_TWO,
                                  app_exists=True, fly_org="")
        self.assertEqual(proc.returncode, 0, proc.stderr)


if __name__ == "__main__":
    unittest.main()
