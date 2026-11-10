#!/usr/bin/env bash

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=./common.sh
source "$SCRIPT_DIR/common.sh"
# shellcheck source=../egress_host_deps.sh
source "$SCRIPT_DIR/../egress_host_deps.sh"

# Readiness probe budget: ~60s of tunnel reconnect, then fail. Overridable so
# the suite can exercise the retry without sleeping through it.
READYZ_ATTEMPTS="${RUNNER_READYZ_ATTEMPTS:-6}"
READYZ_RETRY_SECONDS="${RUNNER_READYZ_RETRY_SECONDS:-10}"
readonly READYZ_ATTEMPTS READYZ_RETRY_SECONDS
readonly READYZ_TIMEOUT_SECONDS=10

verify_files_and_service() {
  runner_remote "
    set -e
    test \"\$(stat -c %a $HOST_ENV_FILE)\" = 600
    test \"\$(stat -c %a $UNIT_ENV_FILE)\" = 600
    test -x $HOST_DEPLOY_DIR/deploy.sh
    test -x $INSTALL_DIR/$BINARY_NAME
    test -r $UNIT_STREAM_CA_FILE
    sudo grep -q '^AGENTSFLEET_RUNNER_STREAM_URL=' $UNIT_ENV_FILE
    test -f $SYSTEMD_DIR/$SERVICE_NAME
    systemctl is-enabled --quiet $SERVICE_NAME
    test \"\$(systemctl is-active $SERVICE_NAME)\" = active
    test \"\$(systemctl show $SERVICE_NAME --property=Delegate --value)\" = yes
    test \"\$(systemctl show $SERVICE_NAME --property=DelegateSubgroup --value)\" = runner
  "
}

verify_cgroup_controllers() {
  local report service_subtree
  report="$(runner_remote '
    set -e
    cgroup_path="$(systemctl show '"$SERVICE_NAME"' --property=ControlGroup --value)"
    test "$cgroup_path" = /system.slice/'"$SERVICE_NAME"'
    root_path='"$CGROUP_ROOT"'
    slice_path="$root_path/system.slice"
    service_path="$root_path$cgroup_path"
    for entry in \
      "root:$root_path" \
      "slice:$slice_path" \
      "service:$service_path"
    do
      label="${entry%%:*}"
      path="${entry#*:}"
      printf "%s_controllers=" "$label"
      sudo cat "$path/cgroup.controllers"
      printf "%s_subtree=" "$label"
      sudo cat "$path/cgroup.subtree_control"
    done
  ')"
  service_subtree="$(
    printf '%s\n' "$report" |
      sed -n 's/^service_subtree=//p'
  )"

  local controller
  for controller in $REQUIRED_CGROUP_CONTROLLERS; do
    case " $service_subtree " in
      *" $controller "*) ;;
      *)
        echo "ERROR: cgroup controller is not delegated: $controller" >&2
        printf '%s\n' "$report" >&2
        return 1
        ;;
    esac
  done
}

# Without IPv4 forwarding the runner's egress probe fails, and the daemon marks
# every runner assigned an allowlisted egress policy degraded and leases it
# nothing, while the service itself reads healthy.
verify_ipv4_forwarding() {
  local forwarding
  forwarding="$(runner_remote "cat '$IPV4_FORWARD_PROC'" || true)"
  if [ "$forwarding" != 1 ]; then
    echo "ERROR: $IPV4_FORWARD_PROC reads '${forwarding:-unreadable}', so the runner cannot enforce allowlisted egress; the runner deploy turns it on in $IPV4_FORWARD_SYSCTL_FILE, which a later sysctl.d file can override" >&2
    return 1
  fi
  echo "  ✓ IPv4 forwarding: on"
}

# The runner reaches the control plane through Cloudflare, so this probe reads
# the edge as well as the box. A cloudflared connector re-registers for a few
# seconds after every API deploy, and Cloudflare answers 530 (error 1033 — no
# connector for the hostname) for that whole window. The development lane ships
# the API and the runner in the same run, so a single-shot curl lands inside it
# and fails a runner that is fine: run 32385110520 died that way one second
# after the deploy reported the service active, with /readyz green moments
# later. Retry over a bounded window and name the last status, so a genuine
# outage still fails loud — and says what the edge answered.
verify_control_plane() {
  local attempt=1 status=""
  while [ "$attempt" -le "$READYZ_ATTEMPTS" ]; do
    status="$(
      runner_remote "curl -sS -o /dev/null -m $READYZ_TIMEOUT_SECONDS -w '%{http_code}' '$RUNNER_API_URL/readyz' || true"
    )"
    if [ "$status" = 200 ]; then
      echo "  ✓ control plane: $RUNNER_API_URL/readyz 200 (attempt $attempt/$READYZ_ATTEMPTS)"
      return 0
    fi
    echo "  … control plane: $RUNNER_API_URL/readyz answered ${status:-000} (attempt $attempt/$READYZ_ATTEMPTS)"
    attempt=$((attempt + 1))
    if [ "$attempt" -le "$READYZ_ATTEMPTS" ]; then
      sleep "$READYZ_RETRY_SECONDS"
    fi
  done
  echo "ERROR: control plane unreachable from $RUNNER_ITEM after $READYZ_ATTEMPTS attempts: $RUNNER_API_URL/readyz answered ${status:-000}" >&2
  return 1
}

# These probes open no authenticated session: verify the private route and
# each hybrid TLS carrier; the existing heartbeat check proves framed traffic.
verify_private_stream() {
  [ "$ENV" = dev ] || return 0
  runner_remote "getent ahosts agentsfleetd-dev.internal >/dev/null"
  runner_remote "curl -sf -m $READYZ_TIMEOUT_SECONDS http://agentsfleetd-dev.internal:3000/readyz >/dev/null"
  runner_require_native_tls
  local carrier protocol version group_line
  for carrier in h2 h3; do
    protocol=-tls1_3 version=TLSv1.3 group_line=1
    # OpenSSL rejects -quic combined with -tls1_3; QUIC already uses TLS 1.3.
    # Over QUIC, s_client reports the version as QUICv1 and prints no group
    # line at all, with or without -brief (checked with OpenSSL 3.6 against a
    # public QUIC endpoint). The hybrid proof there is the offer itself:
    # -groups X25519MLKEM768 is the only key share sent, so the handshake
    # completes only if the daemon accepts that group.
    if [ "$carrier" = h3 ]; then
      protocol=-quic version=QUICv1 group_line=0
    fi
    if ! runner_remote "
      set -e
      summary=\$(timeout $READYZ_TIMEOUT_SECONDS openssl s_client -brief -verify_return_error \
        -verify_hostname agentsfleetd-dev.internal -servername agentsfleetd-dev.internal \
        -CAfile '$UNIT_STREAM_CA_FILE' -groups X25519MLKEM768 \
        $protocol -alpn $carrier -connect agentsfleetd-dev.internal:4443 </dev/null 2>&1)
      printf '%s\\n' \"\$summary\" |
        grep -E '^(CONNECTION ESTABLISHED|Protocol version:|Verification:|Negotiated TLS1.3 group:|Peer Temp Key:)'
      printf '%s\\n' \"\$summary\" | grep -Fx 'CONNECTION ESTABLISHED' >/dev/null
      printf '%s\\n' \"\$summary\" | grep -Fx 'Protocol version: $version' >/dev/null
      printf '%s\\n' \"\$summary\" | grep -Fx 'Verification: OK' >/dev/null
      [ $group_line = 0 ] || printf '%s\\n' \"\$summary\" |
        grep -E '^(Negotiated TLS1.3 group:|Peer Temp Key:) X25519MLKEM768(,.*)?$' >/dev/null
    "; then
      echo "ERROR: private runner $carrier TLS/ALPN connection failed; check the route/listener and OpenSSL 3.5+ QUIC/hybrid support" >&2
      return 1
    fi
    echo "  ✓ private runner $carrier: verified $version with a hybrid-only offer, ALPN $carrier offered"
  done
}

# The host's own token, proved against the control plane: `GET /v1/runners/me`
# answers 200 only for a token the daemon accepts, and reading it moves no
# liveness, so the check cannot mask a dead runner. The token never leaves the
# host: the remote shell reads it from the unit's environment file, and curl's
# error line names the status, never the header.
verify_runner_identity() {
  runner_remote "
    set -e
    response=\$(sudo sh -c 'set -a; . $UNIT_ENV_FILE; set +a; curl -fsS -m $READYZ_TIMEOUT_SECONDS -H \"Authorization: Bearer \$AGENTSFLEET_RUNNER_TOKEN\" \"\$AGENTSFLEET_API_URL/v1/runners/me\"')
    printf '%s\\n' \"\$response\" |
      jq -e '.last_seen_at | sub(\"\\\\.[0-9]+Z$\"; \"Z\") | fromdateiso8601 | now - . | . >= -5 and . <= 20' >/dev/null
  " || {
    echo 'ERROR: runner authentication or recent native stream heartbeat verification failed' >&2
    return 1
  }
}

main() {
  runner_load_target
  echo "Verifying $RUNNER_ITEM in ${ENV} via Tailscale SSH"

  runner_require_tailnet_online
  egress_probe_remote runner_remote
  verify_ipv4_forwarding
  verify_files_and_service
  verify_cgroup_controllers
  verify_control_plane
  verify_private_stream
  verify_runner_identity

  echo "PASS: $RUNNER_ITEM is ready"
}

main "$@"
