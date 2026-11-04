FROM python:3.12-slim@sha256:b18b261e1b4974b4f321451ad5251ded85abb22c2b8b262da527dda1d031ab30
RUN apt-get update && apt-get install --no-install-recommends -y nftables util-linux iproute2 && apt-get clean
WORKDIR /repository
ENV PYTHONDONTWRITEBYTECODE=1
ENTRYPOINT ["python3", "scripts/runner_collector_admission_test.py"]
