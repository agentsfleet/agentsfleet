"""Pins the collector wiring in the deploy workflows.

Every assertion here corresponds to a defect that actually shipped into the
branch and was caught in review, not to a hypothetical. All three passed
actionlint, YAML parsing and every other mechanical gate, because each was a
correctly-spelled variable that meant the wrong thing or a correctly-formed
step in the wrong place.

  * the daemon staged the VENDOR endpoint, so it never traversed the collector
  * the collector staged its OWN address upstream, so it exported to itself
  * production deployed the daemon BEFORE confirming the collector was up
"""

import unittest
from pathlib import Path

import yaml

REPO = Path(__file__).resolve().parent.parent
WORKFLOWS = REPO / ".github" / "workflows"

# job, deploy-step name, collector app
ENVIRONMENTS = [
    ("deploy-dev-fly.yml", "deploy-fly", "Deploy agentsfleetd-dev", "otelcol-dev"),
    ("release.yml", "deploy-fly-prod", "Deploy PROD API to Fly.io", "otelcol-prod"),
]

ENSURE_STEP = "Ensure the OTLP collector is running"
STAGE_STEP = "Stage Fly secrets from vault"


def steps_of(workflow, job):
    doc = yaml.safe_load((WORKFLOWS / workflow).read_text())
    return doc["jobs"][job]["steps"]


def index_of(steps, needle):
    for i, step in enumerate(steps):
        if needle in (step.get("name") or ""):
            return i
    raise AssertionError(f"no step named like {needle!r}")


class CollectorWiringTest(unittest.TestCase):
    def test_daemon_endpoint_points_at_the_collector(self):
        # The bug: this staged "$GRAFANA_OTLP_ENDPOINT", the vendor. The hop
        # then existed in the diff and never in the traffic.
        for wf, job, _, _ in ENVIRONMENTS:
            with self.subTest(wf):
                steps = steps_of(wf, job)
                run = steps[index_of(steps, STAGE_STEP)]["run"]
                self.assertIn('GRAFANA_OTLP_ENDPOINT="$OTLP_COLLECTOR_ENDPOINT"', run)

    def test_collector_upstream_points_at_the_vendor(self):
        # The mirror bug: this staged "$OTLP_COLLECTOR_ENDPOINT", so the
        # collector's exporter addressed the collector.
        for wf, job, _, _ in ENVIRONMENTS:
            with self.subTest(wf):
                steps = steps_of(wf, job)
                run = steps[index_of(steps, ENSURE_STEP)]["run"]
                self.assertIn('GRAFANA_OTLP_ENDPOINT="$GRAFANA_OTLP_ENDPOINT"', run)
                self.assertNotIn(
                    'GRAFANA_OTLP_ENDPOINT="$OTLP_COLLECTOR_ENDPOINT"', run
                )

    def test_collector_is_ensured_before_the_daemon_deploys(self):
        # The ordering bug: production applied the repointed endpoint before
        # anything confirmed the collector was serving.
        for wf, job, deploy_name, _ in ENVIRONMENTS:
            with self.subTest(wf):
                steps = steps_of(wf, job)
                self.assertLess(
                    index_of(steps, ENSURE_STEP),
                    index_of(steps, deploy_name),
                    "the collector must be serving before the daemon is pointed at it",
                )

    def test_endpoint_names_the_collector_app_it_scales(self):
        # A rename that updated one spelling and not the other would send
        # telemetry to an app nothing is listening on.
        for wf, job, _, app in ENVIRONMENTS:
            with self.subTest(wf):
                doc = yaml.safe_load((WORKFLOWS / wf).read_text())
                env = doc["jobs"][job]["env"]
                self.assertEqual(env["OTLP_COLLECTOR_APP"], app)
                self.assertIn(app, env["OTLP_COLLECTOR_ENDPOINT"])
                self.assertIn(app, env["OTLP_COLLECTOR_DIR"])

    def test_receiver_requires_authentication(self):
        # Without this the receiver is a credentialed relay open to every
        # workload on an organisation-wide private network.
        for _, _, _, app in ENVIRONMENTS:
            with self.subTest(app):
                cfg = yaml.safe_load((REPO / "deploy" / "fly" / app / "config.yml").read_text())
                http = cfg["receivers"]["otlp"]["protocols"]["http"]
                self.assertEqual(http["auth"]["authenticator"], "basicauth/ingest")
                self.assertIn("basicauth/ingest", cfg["service"]["extensions"])

    def test_collector_adds_no_attributes_to_any_pipeline(self):
        # Continuity is the deliverable: a collector that decorates a series
        # makes a dashboard gap indistinguishable from a daemon regression.
        forbidden = {"attributes", "resource", "transform", "filter"}
        for _, _, _, app in ENVIRONMENTS:
            cfg = yaml.safe_load((REPO / "deploy" / "fly" / app / "config.yml").read_text())
            for name, pipeline in cfg["service"]["pipelines"].items():
                with self.subTest(app=app, pipeline=name):
                    used = {p.split("/")[0] for p in pipeline["processors"]}
                    self.assertEqual(used & forbidden, set())

    def test_every_signal_has_a_pipeline(self):
        for _, _, _, app in ENVIRONMENTS:
            cfg = yaml.safe_load((REPO / "deploy" / "fly" / app / "config.yml").read_text())
            with self.subTest(app):
                self.assertEqual(
                    set(cfg["service"]["pipelines"]), {"logs", "traces", "metrics"}
                )


if __name__ == "__main__":
    unittest.main()
