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

Passed. The kernel version came from inside the guest twice. The guest's `/init` printed `kernel=6.18.62`, read from `/proc/sys/kernel/osrelease`, to the serial console. A command spawned through the executor over vsock printed `6.18.62` from `uname -r`. The host's own kernel is `6.12.111+deb13-amd64`, so neither answer can have come from the host.

20 boots ran in sequence on a fresh workspace image, with no other VM running. Each was timed from process start until the guest printed that it listened on vsock, and until the executor answered `list_dir` through the host's vsock socket:

| | guest ready | executor answers | VMM resident set size (RSS) |
|---|---|---|---|
| min | 175 ms | 186 ms | 115,600 KiB |
| median | 194 ms | 201 ms | 115,608 KiB |
| max | 218 ms | 230 ms | 109,464 KiB |

Each row is the boot with the minimum, median or maximum executor time, and its RSS is that boot's. Bubblewrap's documented start is "tens of milliseconds" (`afr_sandbox/src/engine/mod.rs:30-32`); a VM is about 200 ms.

**Idle memory by guest size**, one VM at a time, read 2 s after the executor answered:

| Guest memory | VMM RSS | Used inside the guest (MemTotal − MemFree) |
|---|---|---|
| 512 MiB | 80.9 MiB | 26.2 MiB |
| 1024 MiB | 92.9 MiB | 45.9 MiB |
| 2048 MiB | 110.9 MiB | 46.2 MiB |
| 4096 MiB | 220.9 MiB | 51.6 MiB |

The guest kernel already carries `CONFIG_VIRTIO_BALLOON=y` and `CONFIG_PAGE_REPORTING=y`. Whether a balloon device with free page reporting lowers the host's RSS was not measured.

## Dimension 1.2: the executor answers through the vsock handshake

Passed. `CONNECT 1024` answered `"OK 1073741824"`, and `list_dir .` answered `["lost+found"]` in 4 ms. `CONNECT 1025`, a port no one listens on, answered `""` with no `OK`, and the client exited 1. A spawned `sh -c` ended `Exited(0)` in 32 ms.

The entry was a spike helper that links the real executor crate, not the runner's own binary: the runner has no guest mode yet, and building one is §7. The helper hands the vsock descriptor to the executor as a stream, which §3's `Transport` replaces.

## Dimension 1.3: a stopped VM loses no synced byte

Passed, on a fresh image with no other VM running. With `vdb` on `cache_type` `Writeback`:

1. The guest wrote 1 MiB of random bytes, `sha256sum` printed `4d290fd2…deb2`, and the guest ran `sync`.
2. The VMM was stopped with `SIGKILL`, and `e2fsck -fn` on the image reported no errors.
3. A fresh VM booted on the same image and read `4d290fd2…deb2  /workspace/probe.bin`.

An unsynced write, `cde50ee4…`, killed the same way, was gone after the reboot: `No such file or directory`. `e2fsck -fn` reported no errors after every kill. No boot made an API call; every VM ran from `--config-file`.

A `Stop` hold must therefore sync the guest before the VMM stops, as §7's hold says.

**A first run that does not count.** A VM left by a failed first script kept `workspace.ext4` mounted while the first 20 boots and both kills ran, so two VMs wrote one ext4 image. The guest console then read `EXT4-fs error (device vdb): ext4_lookup:1789: inode #13: … iget: checksum invalid`. That run was discarded, and every number above is from the re-run. It is also a finding: nothing stops a second VMM from attaching a workspace image a live VM holds, so §7 must.

## Who can reach the executor

- **On the host, only root.** The vsock socket Firecracker made was `srwxr-xr-x root:root`. Connecting to a Unix socket needs write permission on it, and `/root` is mode `700`.
- **From inside the guest, nothing.** The guest kernel has `# CONFIG_VSOCKETS_LOOPBACK is not set`, so a guest process cannot dial the guest's own listener.
- **From another VM or the network, nothing.** Each VM's vsock device is its own, and no TCP port was opened.

Under §7 the socket lives in each jail, owned by that jail's user.

## What the spike did not cover

- **The jailer.** Every boot ran Firecracker directly as root, so the jailer's user, chroot, cgroup leaf and network namespace are §7's to prove.
- **Networking.** The VMs had no tap device, so egress is §4's and §7's to prove.
- **Concurrent boots.** Boots were sequential, so start time under load is unmeasured.
- **Memory under load.** Idle RSS was about 111 MiB per 2 GiB VM. §8's overcommit at 4 to 1 assumes running guests touch well under their 2 GiB, which no working set has measured yet.

## For D15, the pool

A pooled VM saves about 200 ms of the start (median 201 ms to the executor's answer), and an idle pooled 2 GiB VM holds about 111 MiB of host memory. Indy ruled a pool of four (D15) with these numbers unmeasured. They are recorded here for any re-ruling.
