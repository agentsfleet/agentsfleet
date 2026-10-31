#!/usr/bin/env bash
# Chromium reads the user's NSS database; Bun uses the caller's extra CA bundle.
acceptance_certificate_added=0

acceptance_trust_certificate() {
  local certificate="$1" database="${2:-$HOME/.pki/nssdb}"
  acceptance_certificate_database="$database"
  acceptance_certificate_name="agentsfleet-local-$DEV_ID"
  mkdir -p "$database"
  if [ ! -f "$database/cert9.db" ]; then certutil -N --empty-password -d "sql:$database"; fi
  if certutil -L -d "sql:$database" -n "$acceptance_certificate_name" >/dev/null 2>&1; then
    printf 'Local acceptance certificate name already exists; refusing to replace its trust.\n' >&2
    return 1
  fi
  certutil -A -d "sql:$database" -n "$acceptance_certificate_name" -t 'C,,' -i "$certificate" || return $?
  acceptance_certificate_added=1
}

acceptance_untrust_certificate() {
  local certificate="$1" current expected
  if [ "$acceptance_certificate_added" = 1 ]; then
    expected="$(openssl x509 -in "$certificate" -noout -fingerprint -sha256)" || return $?
    current="$(certutil -L -d "sql:$acceptance_certificate_database" -n "$acceptance_certificate_name" -a | openssl x509 -noout -fingerprint -sha256)" || return $?
    [ "$current" = "$expected" ] || { printf 'Local acceptance trust identity changed; refusing to remove it.\n' >&2; return 1; }
    certutil -D -d "sql:$acceptance_certificate_database" -n "$acceptance_certificate_name" || return $?
    acceptance_certificate_added=0
  fi
}
