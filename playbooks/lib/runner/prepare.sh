#!/usr/bin/env bash

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=./common.sh
source "$SCRIPT_DIR/common.sh"
# Where the guest set is installed on a host (GUEST_IMAGE_DIR_DEFAULT).
# shellcheck source=../../../deploy/baremetal/toolbox.sh
source "$SCRIPT_DIR/../../../deploy/baremetal/toolbox.sh"

require_approvals() {
  playbooks_require_vault_read_approval
  if [ "${ALLOW_RUNNER_HOST_PREPARE:-0}" != "1" ]; then
    echo "ERROR: runner host preparation requires Human approval" >&2
    exit 1
  fi
}

install_host_dependencies() {
  runner_remote '
    set -e
    sudo apt-get update -qq
    sudo apt-get install -y --no-install-recommends \
      nftables iproute2 ca-certificates git openssl curl jq
  '
  if [ "$ENV" = dev ]; then
    runner_remote 'sudo apt-get install -y --no-install-recommends wireguard-tools'
  fi
}

# What the host's kernel must offer a Firecracker microVM: the Kernel-based
# Virtual Machine (KVM) device its monitor opens, loaded for this processor's
# vendor, and the tap device its network rides on. Both are loaded now and at
# every boot. The guest's vsock ends in a Unix socket the monitor serves, so
# the host loads no vsock module.
readonly HOST_MODULES_FILE="/etc/modules-load.d/agentsfleet-runner.conf"

load_host_modules() {
  runner_remote "
    set -e
    case \"\$(grep -o -w -E 'vmx|svm' /proc/cpuinfo | head -n 1)\" in
      vmx) vendor=kvm_intel ;;
      svm) vendor=kvm_amd ;;
      *)
        echo 'ERROR: this host offers no hardware virtualization: /proc/cpuinfo names neither vmx nor svm' >&2
        exit 1
        ;;
    esac
    printf '%s\n' kvm \"\$vendor\" tun | sudo tee '$HOST_MODULES_FILE' >/dev/null
    sudo modprobe -a kvm \"\$vendor\" tun
    for device in /dev/kvm /dev/net/tun; do
      if [ ! -c \"\$device\" ]; then
        echo \"ERROR: \$device is missing after loading kvm, \$vendor and tun\" >&2
        exit 1
      fi
    done
  "
}

# Where deploy.sh installs the guest kernel and initramfs (toolbox.sh's
# GUEST_IMAGE_DIR_DEFAULT): root's, readable by all, made here so a host is
# whole before its first deploy.
prepare_host_paths() {
  runner_remote "
    set -e
    sudo mkdir -p $HOST_STAGING_DIRS
    sudo chown -R '$RUNNER_USER:$RUNNER_USER' $HOST_ROOT
    sudo install -d -m 755 -o 0 -g 0 '$GUEST_IMAGE_DIR_DEFAULT'
  "
}

main() {
  require_approvals
  runner_load_target

  echo "Preparing $RUNNER_ITEM in ${ENV} via Tailscale SSH"
  runner_verify_host_cgroup_capability
  install_host_dependencies
  load_host_modules
  runner_require_native_tls
  runner_enable_ipv4_forwarding
  prepare_host_paths
  runner_require_tailnet_online
  runner_require_remote_tools
  echo "PASS: $RUNNER_ITEM host preparation completed"
}

main "$@"
