FROM otel/opentelemetry-collector-contrib:0.164.0@sha256:340c9ca6a36617084c6661bdc4f0fe92b7cda5a805d61673ec8db375fddc522b AS collector
FROM python:3.12-slim@sha256:b18b261e1b4974b4f321451ad5251ded85abb22c2b8b262da527dda1d031ab30
COPY --from=collector /otelcol-contrib /usr/local/bin/otelcol-contrib
WORKDIR /repository
ENV PYTHONDONTWRITEBYTECODE=1
ENTRYPOINT ["python3", "scripts/collector_temporality_test.py"]
