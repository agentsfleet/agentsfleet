FROM mirror.gcr.io/otel/opentelemetry-collector-contrib:latest

# config.yml defines the receive -> batch -> forward path. The contrib
# distribution is required for the basicauth extension Grafana Cloud's OTLP
# endpoint needs; the core distribution does not carry it.
#
# Fly secrets, set by the deploy workflow from the vault:
#   GRAFANA_OTLP_ENDPOINT / GRAFANA_OTLP_INSTANCE_ID / GRAFANA_OTLP_API_KEY
# These are the COLLECTOR's upstream credentials. The daemon keeps its own
# copy pointed at this app instead, which is the whole repoint.
COPY config.yml /etc/otelcol-contrib/config.yaml
