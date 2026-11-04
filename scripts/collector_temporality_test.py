"""Exercise both declared collector metrics pipelines with structured OTLP fixtures."""

import argparse
import contextlib
import http.server
import json
import os
import pathlib
import queue
import socket
import subprocess
import tempfile
import threading
import time
import unittest
import urllib.error
import urllib.request

ROOT = pathlib.Path(__file__).resolve().parent.parent
BINARY = "otelcol-contrib"
CONFIG_ROOT = ROOT
FIXTURES = {"GRAFANA_OTLP_ENDPOINT": "https://example.invalid/otlp",
    "GRAFANA_OTLP_INSTANCE_ID": "fixture", "GRAFANA_OTLP_API_KEY": "fixture",
    "OTLP_INGEST_USER": "fixture", "OTLP_INGEST_PASSWORD": "fixture"}


class Receiver(http.server.BaseHTTPRequestHandler):
    def do_POST(self):
        body = self.rfile.read(int(self.headers["Content-Length"]))
        self.server.exports.put(json.loads(body))
        self.send_response(200)
        self.send_header("Content-Type", "application/json")
        self.end_headers()
        self.wfile.write(b"{}")

    def log_message(self, *_args):
        pass


def window(index):
    count = index + 2
    stamp = {"startTimeUnixNano": str(1_700_000_000_000_000_000 + index * 1_000_000_000),
             "timeUnixNano": str(1_700_000_000_000_000_000 + (index + 1) * 1_000_000_000),
             "attributes": [{"key": "stage", "value": {"stringValue": "event_to_complete"}}]}
    metrics = [
        {"name": "agentsfleet_fleet_delivery_stage_seconds", "unit": "s", "histogram": {
            "aggregationTemporality": 1, "dataPoints": [{**stamp, "count": str(count),
                "sum": count, "bucketCounts": [str(count), "0"], "explicitBounds": [10]}]}},
        {"name": "agentsfleet.billing.credit.consumed", "sum": {"isMonotonic": True,
            "aggregationTemporality": 1, "dataPoints": [{**stamp, "asInt": str(count)}]}},
        {"name": "agentsfleet_runner_wake_answers_total", "sum": {"isMonotonic": True,
            "aggregationTemporality": 2, "dataPoints": [{**stamp, "asInt": str(7 + index * 2)}]}},
        {"name": "agentsfleet_api_in_flight_requests", "gauge": {
            "dataPoints": [{**stamp, "asInt": str(11 + index * 2)}]}},
    ]
    return {"resourceMetrics": [{"scopeMetrics": [{"metrics": metrics}]}]}


@contextlib.contextmanager
def collector_once(environment):
    path = CONFIG_ROOT / "deploy" / "fly" / ("otelcol-" + environment) / "config.yml"
    config = json.loads(subprocess.check_output([BINARY, "print-config", "--format=json", "--config=" + str(path)],
        env={**os.environ, **FIXTURES}, text=True))
    with http.server.ThreadingHTTPServer(("127.0.0.1", 0), Receiver) as sink:
        sink.exports = queue.Queue()
        thread = threading.Thread(target=sink.serve_forever, daemon=True)
        thread.start()
        with socket.socket() as reserve:
            reserve.bind(("127.0.0.1", 0))
            incoming_port = reserve.getsockname()[1]
        endpoint = "http://127.0.0.1:" + str(incoming_port)
        # Replace only transport and batching cadence; execute the configured processors/order.
        config["receivers"] = {"otlp": {"protocols": {"http": {"endpoint": "127.0.0.1:" + str(incoming_port)}}}}
        config["exporters"] = {"otlphttp/test": {"endpoint": "http://127.0.0.1:" + str(sink.server_port),
            "encoding": "json", "compression": "none", "sending_queue": {"enabled": False}}}
        config["processors"]["batch"]["timeout"] = "50ms"
        pipeline = config["service"]["pipelines"]["metrics"]
        config["service"] = {"pipelines": {"metrics": {**pipeline, "exporters": ["otlphttp/test"]}}}
        config.pop("extensions", None)
        with tempfile.TemporaryDirectory(prefix="collector-temporality-") as directory:
            candidate = pathlib.Path(directory) / "config.json"
            with candidate.open("w", encoding="utf-8") as output:
                json.dump(config, output)
            with tempfile.TemporaryFile(mode="w+") as logs:
                process = subprocess.Popen([BINARY, "--config=" + str(candidate)], stdout=logs, stderr=logs)
                try:
                    deadline = time.monotonic() + 10
                    while time.monotonic() < deadline:
                        if process.poll() is not None:
                            logs.seek(0)
                            raise RuntimeError("collector exited: " + logs.read())
                        try:
                            urllib.request.urlopen(endpoint, timeout=.2).close()
                        except urllib.error.HTTPError:
                            break
                        except (urllib.error.URLError, ConnectionError):
                            time.sleep(.05)
                    else:
                        raise TimeoutError("collector receiver did not become ready")
                    yield endpoint, sink.exports
                finally:
                    process.terminate()
                    try:
                        process.wait(timeout=10)
                    except subprocess.TimeoutExpired:
                        process.kill()
                        process.wait()
                    sink.shutdown()
                    thread.join(timeout=5)


@contextlib.contextmanager
def collector(environment):
    # An external server cannot inherit the reserved socket; retry a bind race.
    with contextlib.ExitStack() as owned:
        for attempt in range(3):
            try:
                ready = owned.enter_context(collector_once(environment))
                break
            except RuntimeError as error:
                if "address already in use" not in str(error) or attempt == 2:
                    raise
        yield ready


def exported_windows(environment):
    result = []
    with collector(environment) as (endpoint, exports):
        invalid = urllib.request.Request(endpoint + "/v1/metrics", data=b'{"resourceMetrics":"invalid"}',
            headers={"Content-Type": "application/json"})
        try:
            urllib.request.urlopen(invalid, timeout=5).close()
        except urllib.error.HTTPError as error:
            rejection = (error.code, json.loads(error.read()))
        else:
            raise AssertionError("malformed metric input was accepted")
        if not exports.empty():
            raise AssertionError("malformed input reached the exporter")
        for index in range(2):
            request = urllib.request.Request(endpoint + "/v1/metrics", data=json.dumps(window(index)).encode(),
                headers={"Content-Type": "application/json"})
            with urllib.request.urlopen(request, timeout=5) as response:
                if response.status != 200:
                    raise AssertionError("collector refused metrics: " + str(response.status))
            payload = exports.get(timeout=10)
            result.append({metric["name"]: metric for resource in payload["resourceMetrics"]
                for scope in resource["scopeMetrics"] for metric in scope["metrics"]})
    return result, rejection


class CollectorTemporality(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.observed = {environment: exported_windows(environment) for environment in ("dev", "prod")}

    def test_delta_windows_accumulate(self):
        for environment, (windows, _rejection) in self.observed.items():
            for index, metrics in enumerate(windows):
                with self.subTest(environment=environment, window=index):
                    count = (2, 5)[index]
                    histogram = metrics["agentsfleet_fleet_delivery_stage_seconds"]
                    point = histogram["histogram"]["dataPoints"][0]
                    cost = metrics["agentsfleet.billing.credit.consumed"]["sum"]
                    self.assertEqual({"unit": histogram["unit"], "temporality": histogram["histogram"]["aggregationTemporality"],
                        "count": int(point["count"]), "sum": point["sum"], "bounds": point["explicitBounds"],
                        "buckets": list(map(int, point["bucketCounts"])), "attributes": point["attributes"],
                        "cost_temporality": cost["aggregationTemporality"], "cost": int(cost["dataPoints"][0]["asInt"])},
                        {"unit": "s", "temporality": 2, "count": count, "sum": count, "bounds": [10], "buckets": [count, 0],
                        "attributes": [{"key": "stage", "value": {"stringValue": "event_to_complete"}}],
                        "cost_temporality": 2, "cost": count})

    def test_cumulative_and_gauge_are_unchanged(self):
        for environment, (windows, _rejection) in self.observed.items():
            for index, metrics in enumerate(windows):
                with self.subTest(environment=environment, window=index):
                    wake = metrics["agentsfleet_runner_wake_answers_total"]["sum"]
                    gauge = metrics["agentsfleet_api_in_flight_requests"]["gauge"]
                    self.assertEqual((set(metrics), wake["aggregationTemporality"], int(wake["dataPoints"][0]["asInt"]),
                        int(gauge["dataPoints"][0]["asInt"])),
                        ({"agentsfleet_fleet_delivery_stage_seconds", "agentsfleet.billing.credit.consumed",
                          "agentsfleet_runner_wake_answers_total", "agentsfleet_api_in_flight_requests"}, 2, 7 + index * 2, 11 + index * 2))

    def test_full_configurations_validate(self):
        for environment in ("dev", "prod"):
            with self.subTest(environment=environment):
                path = CONFIG_ROOT / "deploy" / "fly" / ("otelcol-" + environment) / "config.yml"
                result = subprocess.run([BINARY, "validate", "--config=" + str(path)],
                    env={**os.environ, **FIXTURES}, capture_output=True, text=True, check=False)
                self.assertEqual(result.returncode, 0, result.stderr)

    def test_malformed_input_is_rejected_before_export(self):
        for environment, (_windows, rejection) in self.observed.items():
            with self.subTest(environment=environment):
                status, body = rejection
                self.assertEqual((status, body["code"]), (400, 3))
                self.assertTrue(body["message"], "OTLP rejection must describe the invalid input")


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--collector", default=BINARY)
    parser.add_argument("--config-root", type=pathlib.Path, default=CONFIG_ROOT)
    arguments = parser.parse_args()
    BINARY, CONFIG_ROOT = arguments.collector, arguments.config_root
    unittest.main(argv=[__file__], verbosity=2)
