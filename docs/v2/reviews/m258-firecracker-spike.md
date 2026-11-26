# M258_001 §1 spike: one Firecracker VM booted by hand on the dev host

**Date:** Nov 26, 2026

**Host:** `zombie-dev-worker-ant`, the dev runner host

**Approved by:** Indy, Nov 26, 2026: "yeah use the latest firecraker and so on"

**Scope:** the spike ran in `/root/m258-spike`, a scratch directory on the dev host. It left the runner service, its state directory and the host's settings untouched (`systemctl is-active agentsfleet-runner` read `active` before and after). The helpers live outside the repository and are deleted when the stream ends.

## Host

| Fact | Value | Source |
|---|---|---|
| Kernel | `6.12.111+deb13-amd64`, Debian 13, unchanged (D6) | `uname -r` |
| Processors | 8, two threads per core: simultaneous multithreading (SMT) on (D7) | `nproc`; `lscpu` |
| Memory | 31.2 GiB total, 30.4 GiB available | `/proc/meminfo` |
| Devices | `/dev/kvm`, `/dev/vhost-vsock` (`root:kvm`), `/dev/net/tun` | `ls -l` |
| Modules | `kvm_intel`, `erofs`, `loop` loaded | `lsmod` |

## What booted

| Piece | Value | Source |
|---|---|---|
| Virtual machine monitor | Firecracker v1.18.0, the latest release (published Nov 09, 2026) | `gh release list --repo firecracker-microvm/firecracker`; `firecracker-v1.18.0-x86_64.tgz: OK` against its `.sha256.txt` |
| Guest kernel | 6.18.62 built from kernel.org source (SHA-256 `490a7aed…` matches `sha256sums.asc`) | `uname -r` in the guest: `6.18.62` |
| Guest kernel configuration | Firecracker's continuous integration (CI) configuration `firecracker-ci/20261125-454a9e84e707-0/x86_64/vmlinux-6.18.62.config`, which ships `# CONFIG_EROFS_FS is not set`, plus `CONFIG_EROFS_FS=y`, `CONFIG_EROFS_FS_ZIP=y`, xattrs, POSIX ACLs and security labels | the build's `grep ^CONFIG_EROFS .config` |
| Guest kernel image | `vmlinux-6.18.62-erofs`, SHA-256 `4d3d798f1838dd1963078853049601d0f8af066de357853e67a4d1495dca93ad`, cross-compiled in a `debian:trixie-slim` container in 4 m 18 s | `sha256sum` on both ends |
| Root | our toolbox, `toolbox-61571fdd….erofs`, SHA-256 `61571fdd560be615f77374a21c507924ab75f6659cd230bd773996835169bef3`, which equals its name, attached read-only as `vda` | `sha256sum` |
| Workspace | a fresh 2 GiB ext4 image as `vdb`, `cache_type` `Writeback` | `vm.json` |
| Guest entry | a one-file initramfs whose `/init` is a static helper linking our `afr_executor` crate. It mounts `vda` as erofs and binds its `/usr` and `/etc`, mounts `vdb` at `/workspace`, and serves the executor on vsock port 1024 | helper SHA-256 `a08bae41…` |
| Size | 2 virtual CPUs, 2048 MiB | `vm.json` |

The guest reported `PRETTY_NAME="Debian GNU/Linux 13 (trixie)"`, `6.18.62`, `2` processors and `git version 2.47.3`, all read through the executor.

## Dimension 1.1: the toolbox boots as a VM's root

Passed. 20 boots in sequence, from process start until the guest printed that it listened on vsock, and until the executor answered `list_dir` through the host's vsock socket:

| | guest ready | executor answers | VMM resident set size (RSS) |
|---|---|---|---|
| min | 184 ms | 188 ms | 111,524 KiB |
| median | 200 ms | 206 ms | 115,604 KiB |
| max | 210 ms | 221 ms | 113,556 KiB |

Each row is the boot with the minimum, median or maximum executor time, and its RSS is that boot's. RSS is the Firecracker process's resident memory just after the answer, idle guest included: about 113 MiB against a 2048 MiB guest. Bubblewrap's documented start is "tens of milliseconds" (`afr_sandbox/src/engine/mod.rs:30-32`); a VM is about 200 ms.

## Dimension 1.2: the executor answers through the vsock handshake

Passed. `CONNECT 1024` answered `"OK 1073741824"`, and `list_dir .` answered `["lost+found"]` in 4 ms. `CONNECT 1025`, a port no one listens on, answered `""` with no `OK`, and the client exited 1. A spawned `sh -c` ended `Exited(0)` in 32 ms.

The entry was a spike helper that links the real executor crate, not the runner's own binary: the runner has no guest mode yet, and building one is §7. The helper hands the vsock descriptor to the executor as a stream, which §3's `Transport` replaces.

## Dimension 1.3: a stopped VM loses no synced byte

Passed. With `vdb` on `cache_type` `Writeback`:

1. The guest wrote 1 MiB of random bytes, `sha256sum` printed `b221506f…9b2a`, and the guest ran `sync`.
2. The VMM was stopped with `SIGKILL`.
3. A fresh VM booted on the same image and read `b221506f…9b2a  /workspace/probe.bin`.

An unsynced write, `bea65e2d…`, killed the same way, was gone after the reboot: `No such file or directory`. No boot made an API call; every VM ran from `--config-file`.

A `Stop` hold must therefore sync the guest before the VMM stops, as §7's hold says.

## What the spike did not cover

- **The jailer.** Every boot ran Firecracker directly as root, so the jailer's user, chroot, cgroup leaf and network namespace are §7's to prove.
- **Networking.** The VMs had no tap device, so egress is §4's and §7's to prove.
- **Concurrent boots.** Boots were sequential, so start time under load is unmeasured.
- **Memory under load.** Idle RSS was about 113 MiB per VM. §8's overcommit at 4 to 1 assumes running guests touch well under their 2 GiB, which no working set has measured yet.

## For D15, the pool

A pooled VM saves about 200 ms of the start (median 206 ms to the executor's answer), and an idle pooled VM holds about 113 MiB. Indy ruled a pool of four (D15) with these numbers unmeasured. They are recorded here for any re-ruling.
