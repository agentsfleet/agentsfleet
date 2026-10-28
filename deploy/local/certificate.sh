#!/usr/bin/env bash
# Temporary localhost-only TLS trust; the caller owns the acceptance lock and exit trap.
acceptance_certificate_added=0

acceptance_trust_certificate() {
  local certificate="$1"
  # Basic trust would bypass the localhost policy; its installer must remove it.
  if security verify-cert -c "$certificate" -p basic >/dev/null 2>&1; then
    printf 'This local certificate already has broad trust. Remove that trust before running acceptance.\n' >&2
    return 1
  fi
  printf 'Temporarily trusting the local Caddy certificate for localhost HTTPS; macOS may ask for approval.\n'
  security add-trusted-cert -r trustRoot -p ssl -s localhost \
    -k "$HOME/Library/Keychains/login.keychain-db" "$certificate" || return $?
  acceptance_certificate_added=1
}

acceptance_untrust_certificate() {
  local certificate="$1"
  if [ "$acceptance_certificate_added" = 1 ]; then
    security remove-trusted-cert "$certificate" || return $?
    acceptance_certificate_added=0
  fi
}
