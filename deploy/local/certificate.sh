#!/usr/bin/env bash
# Trust the local Caddy root for TLS once, in the login keychain, and keep it.
# Chromium skips hostname-scoped keychain trust (net/cert/internal/trust_store_mac.cc),
# so the setting names the TLS policy only. Later runs find it and ask nothing.

# A user trust setting is keyed by the certificate's SHA-1. verify-cert cannot
# answer this: its TLS policy demands certificate transparency a local root never has.
acceptance_certificate_trusted() {
  local certificate="$1" folder digest status=0
  digest="$(openssl x509 -in "$certificate" -noout -fingerprint -sha1)" || return $?
  digest="${digest#*=}"
  folder="$(mktemp -d)" || return $?
  security trust-settings-export "$folder/user.plist" >/dev/null 2>&1 &&
    plutil -p "$folder/user.plist" | grep -qF "\"${digest//:/}\"" || status=1
  rm -r "$folder"
  return "$status"
}

acceptance_trust_certificate() {
  local certificate="$1"
  if acceptance_certificate_trusted "$certificate"; then return 0; fi
  printf 'Trusting the local Caddy certificate for HTTPS; macOS asks for approval once.\n'
  security add-trusted-cert -r trustRoot -p ssl \
    -k "$HOME/Library/Keychains/login.keychain-db" "$certificate"
}

# The trust is kept on purpose; the Linux adapter's removal hook has nothing to do here.
acceptance_untrust_certificate() { :; }
