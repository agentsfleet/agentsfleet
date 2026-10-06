# Toolbox spikes for M211_001 : Oct 04, 2026

M211_001 Discovery requires six spikes before EXECUTE. This page holds their raw results; the spec keeps one line per spike and links here. S1 and S5 ran on `feat/m211-sandbox-tools-and-nested-loops` at `3c8a5d2eb`, through a temporary kernel-lane trial that was never committed. The trial runs one script inside one lease with `Limits::default()`, under the production bubblewrap flags, Landlock and seccomp. S6 ran the same way at `a0c67b2a1`. S2 and S4 ran standalone scripts at `a0c67b2a1`, with the `scripts/toolbox/build.sh` committed beside this page.

## Machines

| Machine | Role | Facts |
|---|---|---|
| `afr-kernel` | kernel lane, the root-mode build, S1, S2, S3, S5, S6 | Ubuntu 24.04.5 arm64 on OrbStack, kernel 7.0.14-orbstack, 7 CPUs, 7 GB of memory; Linux Security Modules (LSM) `capability,landlock,yama,bpf`; cgroup v2 controllers `cpuset cpu io memory pids`; no `/dev/kvm`; rustc 1.98.1, bubblewrap 0.9.0, mmdebstrap 1.4.3, erofs-utils 1.7.1, cryptsetup 2.7.0 |
| `afr-build-a`, `afr-build-b` | the two clean unprivileged builders for S4 | Ubuntu 24.04 arm64 on OrbStack |

## The spike image

`toolbox-222b8018b1a418575bd816c6c1481db0b58dbde0cd814eded4467a22eb33d51f.erofs` holds 871,460,864 bytes and 243 packages. Its Enhanced Read-Only File System (EROFS) features are `sb_csum mtime 0padding`, and it serves runner 0.55.0. It was built from the drafted manifest plus two binaries vendored for S1 only: Codex `rust-v0.160.0` (musl, SHA-256 `88362013…`) and `@anthropic-ai/claude-code-linux-arm64` 2.1.289 (SHA-256 `45ad1d2f…`). Its release manifest pins `chromium-headless-shell` at `151.0.7922.173-1~deb13u1`.

## S1 — the full toolset under the production policy: FAIL, browser only

| Check | Result | Decisive output |
|---|---|---|
| uv installs a locked project from a loopback index | PASS | `+ hello-spike==1.0`, then `hi from a loopback index` |
| `node --test` | PASS | Node v20.19.2: `# pass 1`, `# fail 0` |
| `git commit` | PASS | `b612d2e one` |
| `codex --version` | PASS | `codex-cli 0.160.0` |
| `claude --version` | PASS | `2.1.289 (Claude Code)` |
| `chromium-headless-shell` takes the Chrome DevTools Protocol (CDP) over `--remote-debugging-pipe` and screenshots a loopback page | FAIL | below |

No variant passed `--no-sandbox`, and each stopped before answering `Browser.getVersion`:

```text
default                       ERROR:content/browser/zygote_host/zygote_host_impl_linux.cc:128] No usable sandbox! If this is a Debian system, please install the chromium-sandbox package to solve this problem. […] If you want to live dangerously and need an immediate workaround, you can try using --no-sandbox.
--no-zygote                   ERROR:content/app/content_main_runner_impl.cc:349] Zygote cannot be disabled if sandbox is enabled. Use --no-zygote together with --no-sandbox
--single-process --no-zygote  ERROR:content/app/content_main_runner_impl.cc:349] Zygote cannot be disabled if sandbox is enabled. Use --no-zygote together with --no-sandbox
```

Why it cannot start:

- Chromium's sandbox needs a new user namespace or its setuid `chrome-sandbox` helper. Ours refuses the namespace twice: bubblewrap runs with `--disable-userns` (`rustd/crates/afr_sandbox/src/bubblewrap.rs:70`), and the seccomp filter refuses `unshare` (`rustd/crates/afr_sandbox/src/harden/linux.rs:32`).
- The helper is not in the image (`ls /usr/lib/chromium/chrome-sandbox`: No such file or directory). Debian's `chromium-sandbox` package would add it, and it still could not raise privilege: the toolbox mounts `nosuid` (`rustd/crates/afr_sandbox/src/toolbox.rs:29`), and the process runs under `--cap-drop ALL` (`bubblewrap.rs:71`).
- The host is not the obstacle: `/proc/sys/user/max_user_namespaces` reads 2147483647 inside the lease.
- unverified: that a nested user namespace alone would let Chromium start. The experiment that would settle it, dropping `--disable-userns` and the `unshare` refusal, was refused as a sandbox weakening and never ran.

One false start: the first run failed with `Remote debugging pipe file descriptors are not open`. That was the test driver. `os.pipe()` handed back descriptor 3, and `dup2(3, 3)` changes nothing, so close-on-exec stayed set; `os.set_inheritable` fixed it. An `extra_pipes` implementation has the same trap: a descriptor already at its target number still needs close-on-exec cleared.

**Disposition.** Indy chose "Refuse browser tools, defer §5 (Recommended)" (M211_001 Discovery, Deferrals). The three browser tools answer a code, and §5's design below waits for the Firecracker engine, where each lease has its own kernel. Chromium and its fonts leave the toolbox manifest, and `extra_pipes` leaves `process/spawn`, because nothing else uses them.

**Reactivation.** Once the Firecracker engine runs leases, its milestone repeats S1's browser half inside a microVM and then restores the design below.

### §5 as written at `106d81ef6`

```markdown
### §5 — Browser: Chromium from the toolbox, driven over its pipes

`browser_open { url }` starts Chromium headless from the toolbox through `process/spawn` with `--remote-debugging-pipe` and `extra_pipes: [3, 4]`, then speaks CDP over those pipes to navigate. `browser { action, selector?, text? }` performs `click`, `type`, `text`, `wait` on the open page. `screenshot` captures the page as PNG and attaches it to the next model turn as image content, as §4 does. Chromium runs inside the sandbox under its limits; with the sandbox's network at loopback only, a page beyond loopback fails until the sandbox allowlist lands, and the kernel lane serves its pages on loopback. Chromium exits with the lease. Chromium keeps its own sandbox: `--no-sandbox` is never passed. Spike S1 settles whether it runs inside ours; if it cannot, Indy decides before §5 starts.

- **Dimension 5.1** — `browser_open` loads a loopback page and `text` returns its content → Test `test_browser_opens_and_reads_a_page`
- **Dimension 5.2** — `click` and `type` drive a form and the page reflects it → Test `test_browser_drives_a_form`
- **Dimension 5.3** — `screenshot` returns a PNG that reaches the next turn → Test `test_screenshot_reaches_next_turn`
- **Dimension 5.4** — Chromium holds no capability and a navigation beyond loopback fails → Test `test_browser_inherits_the_sandbox`
- **Dimension 5.5** — Chromium is gone after the lease → Test `test_browser_exits_with_the_lease`

| 5.1 | kernel | `test_browser_opens_and_reads_a_page` | loopback page "hello" → `text` returns `hello` |
| 5.2 | kernel | `test_browser_drives_a_form` | `type` into `#q`, `click` `#go` → page shows the query |
| 5.3 | kernel | `test_screenshot_reaches_next_turn` | screenshot → PNG header, image content on next request |
| 5.4 | kernel | `test_browser_inherits_the_sandbox` | Chromium CapEff 0; navigate `http://1.1.1.1` → error |
| 5.5 | kernel | `test_browser_exits_with_the_lease` | lease ends → no chromium process in the cgroup |

Executor addition: process/spawn { …, extra_pipes: [fd] }
Toolbox packages: chromium-headless-shell fonts-liberation fonts-dejavu-core
Metric: `browser_started` / `browser_exited` (runner log, info) — lease id, milliseconds alive, exit status; no URL, no page text
Reference: Chromium's `--remote-debugging-pipe` — CDP over file descriptors 3 and 4, which is what lets the supervisor drive a browser that has no network and no socket of its own.
```

## S2 — dm-verity beneath EROFS: PASS

The S4 image `377a14f4…` was copied to local disk, a hash tree formatted beside it, and each case opened on its own loop and device-mapper devices. dm-verity is built into this kernel: `/sys/module/dm_verity` is absent, and `veritysetup open` works.

| Field | Value |
|---|---|
| Format | `veritysetup format --hash=sha256 --data-block-size=4096 --hash-block-size=4096`, cryptsetup 2.7.0 |
| Salt | the image's own SHA-256, `377a14f4…` |
| Superblock universally unique identifier (UUID) | `5b2c9a3e-1d4f-4e8a-9c7b-0a6e2f3d4c51`, fixed |
| Data blocks | 63,056 of 4,096 bytes |
| Hash tree | 498 blocks, 2,043,904 bytes: 0.79% of the image |
| Root hash | `fb51494bc2095976311c9c8078efb0943fe10a22b2b63e7941ccf63b6acc4b98` |

Formatting twice gives a byte-identical hash tree, so the root hash can sit in the release record beside the image's digest.

| Case | Without dm-verity | With dm-verity |
|---|---|---|
| One byte flipped in data block 31,528, mid-image and unread at mount | not run | the raw read answers `Input/output error`; the mount succeeds; reading every file, one answers `Input/output error`: `/usr/lib/aarch64-linux-gnu/perl/5.40.1/auto/re/re.so` |
| One byte flipped at offset 512 of block 0, before the superblock, where EROFS checks nothing | the mount succeeds and the change goes unnoticed | the raw read answers `Input/output error`; the mount is refused: `can't read superblock` |

The kernel log names each refusal (`device-mapper: verity: 7:6: data block 0 is corrupted`, then `erofs (device dm-2): cannot read erofs superblock`) and holds no panic, oops or `BUG:` line since boot.

Speed: S1's uv, `node --test` and git steps ran as one sandboxed run (bubblewrap with the production namespace flags, the image as root, a fresh `/workspace`), with the page cache dropped before every run, 20 runs per mount, interleaved:

| Mount | Mean | p50 | p95 | p99 |
|---|---|---|---|---|
| Plain loop device | 315.8 ms | 311.1 ms | 354.3 ms | 371.0 ms |
| dm-verity | 327.4 ms | 326.2 ms | 335.0 ms | 354.0 ms |

The p99 ratio is 0.954 against a limit of 1.10. With 20 runs, p99 is the slowest run, so the median is the steadier reading: dm-verity costs 4.9% there, cold. `drop_caches` empties the page cache; whether it also empties dm-verity's own buffer of hash blocks was not checked.

## S3 — Firecracker boots the exact image: not run

`afr-kernel` has no `/dev/kvm` (`ls /dev/kvm`: No such file or directory), and Firecracker needs it. S3 needs a bare-metal host or a cloud virtual machine with nested virtualization.

## S4 — unprivileged build: PASS

One manifest, three machines, one digest:

| Machine | Mode | Time | Image SHA-256 |
|---|---|---|---|
| `afr-build-a` | `unshare`, unprivileged | 207 s | `377a14f4401c17acc47c6d90f621b550b9e07b49039cb7831eb48a5a55416402` |
| `afr-build-b` | `unshare`, unprivileged | 227 s | the same |
| `afr-kernel` | `root` | 218 s | the same |

The three release manifests match as well (SHA-256 `b096c938…`). The image holds 258,277,376 bytes and 145 packages, with EROFS features `sb_csum mtime 0padding`. The three builds ran at once on one Mac; one at a time, they took 97–139 s.

Three causes kept the earlier attempts apart, each fixed in `scripts/toolbox/build.sh`:

- **No subordinate IDs.** On a builder without `/etc/subuid` and `/etc/subgid` entries, mmdebstrap's automatic mode fell back to a mode that lays the root out differently (`W: /etc/subuid is empty`). The script names `--mode`, so such a builder fails instead.
- **Private directories.** The namespace's root cannot read the builder's 0700 `mktemp` directory, so vendored binaries enter through mmdebstrap's `copy-in` hook and the package list leaves through `download`.
- **The builder's identity.** mmdebstrap 1.4.3 copies the builder's `/etc/hostname` and `/etc/resolv.conf` into the root (`/usr/bin/mmdebstrap:2206`), and its manual prescribes removing both or writing fixed content. The previous three builds (`770e26d3…`, `66634594…`, `82987017…`) differed only in `/etc/hostname`, at block 20. The script now writes `localhost` and an empty file, both mode 0644. They stay as files because the image is read-only and the sandbox's network allowlist will bind rendered resolver files onto them.

Unshare builds print `dpkg: warning: failed to open configuration file '~/.dpkg.cfg' … Permission denied`, because `HOME` leaks into the namespace. The image is unchanged by it.

## S5 — sandbox boundary: PASS, i386 leg not run

The lease's shell holds three descriptors, the executor's pipes, and nothing else:

```text
lr-x------ 1 1000 1000 64 Oct  4 17:24 0 -> pipe:[870790]
l-wx------ 1 1000 1000 64 Oct  4 17:24 1 -> pipe:[870791]
l-wx------ 1 1000 1000 64 Oct  4 17:24 2 -> pipe:[870792]
```

It runs as uid 1000 with every capability set at 0, `NoNewPrivs: 1`, and `Seccomp: 2` with two stacked filters. All 39 namespace requests were refused (`CALLS 39 ALLOWED 0`):

| Call | Flags or target | Answer |
|---|---|---|
| `clone`, `clone3` | `CLONE_NEWUSER` | `ENOSPC`: bubblewrap's `--disable-userns` |
| `unshare` | every `CLONE_NEW*` flag | `EPERM`: the seccomp filter |
| `clone`, `clone3` | `CLONE_NEWNS`, `NEWNET`, `NEWPID`, `NEWIPC`, `NEWUTS`, `NEWCGROUP`, and `NEWTIME` for `clone3` | `EPERM`: no capability |
| `open` before `setns` | `/proc/1/ns/{user,mnt,net,pid,ipc,uts,cgroup,time}` | `EACCES` |
| `setns` | the lease's own `/proc/self/ns/*` | `EINVAL` for `user`; `EPERM` for the other seven |

`clone` and `clone3` are refused by the kernel's own checks, because the seccomp filter lists `unshare` but neither of them (`harden/linux.rs:25-38`).

Not run: the i386-on-amd64 leg needs an amd64 host, and this one is arm64. Apple silicon runs no 32-bit code either, so the arm64 analogue cannot run here.

One false start: the first listing showed a fourth descriptor, `3 -> pipe`. The listing made it: `$(readlink …)` opens a pipe in the very shell it lists. `ls -l /proc/$$/fd` reads the shell's table from another process and adds nothing.

## S6 — writable-state exhaustion: FAIL, two of three criteria

Four leases ran one script at once under `Limits::default()` (2 GiB of memory, a 4 GiB workspace disk): `dd` into `/workspace` until it stops, then the same into `/tmp`. The first attempt is void. Each worker built its own engine, and building an engine sweeps every lease directory in the state directory (`rustd/crates/afr_sandbox/src/bubblewrap_engine/sweep.rs:25`), so three leases lost their workspace image before `mke2fs` ran. The lane also keeps its state in `/tmp` (`rustd/crates/afr_sandbox/examples/kernel_lane/lane.rs:144-145`), a 6.3 GB tmpfs on this host. The rerun shared one engine, as a runner does, and bound a disk directory over `/tmp` in a private mount namespace.

| Lease | `/workspace` | `/tmp` | What the out-of-memory killer took |
|---|---|---|---|
| `s6-0` | `No space left on device`, 4,095,004 KiB used | filled until killed | `dd`, then `bwrap` |
| `s6-1` | killed at 3.81 GB written, before the disk filled | not reached | `bwrap` |
| `s6-2` | `No space left on device` | filled until killed | `dd`, then `bwrap` |
| `s6-3` | `No space left on device` | filled until killed | `dd`, then `bwrap` |

Every lease peaked at its memory limit (`memory.peak 2147483648`) and ended `Interrupted`. The kills come from the kernel log (`Memory cgroup out of memory: Killed process … (bwrap)`); the harness samples `memory.events` every 500 ms and missed `s6-2`'s.

- **Each gets `ENOSPC`: FAIL.** The workspace disk answers `ENOSPC` when its writer lives long enough, but writing to it can exhaust the lease's memory first, as `s6-1` did. unverified: the cause is the loop device's double page cache, with the workspace file system's pages and the backing image's pages both charged to the lease's cgroup; direct I/O on the loop device would remove the second. `/tmp` is bubblewrap's `--tmpfs` at the kernel's default size, 4,098,440 KiB here, larger than the 2 GiB memory limit, so filling it ends in an out-of-memory kill and never `ENOSPC`. bubblewrap 0.9.0 takes `--size` for a tmpfs.
- **The killer takes the sandbox, not only the command.** In all four leases it killed `bwrap`, the sandbox's first process, so one command that fills memory ends the whole sandbox and every process in it.
- **The host keeps its free-space reserve: FAIL.** No reserve exists in the runner's crates or in `docs/architecture/runner_execution.md`. Workspace images are sparse (`rustd/crates/afr_sandbox/src/workspace_disk.rs:1`), so four 4 GiB leases were admitted and then took the state disk from 20 GB free to 5.9 GB. A host with less than 16 GiB free would have filled.
- **Loop-device I/O shows in each lease's cgroup `io.stat`: PASS.** Each lease's `io.stat` carries its own loop device (`7:9` to `7:12`, 3.81–4.19 GB written) and the host disk beneath it (`254:16`).

## Toolbox root against host binds

Asked on Oct 04: does a sandbox rooted in the toolbox image start or run slower than one that binds the host's `/usr`, as the Zig runner's did? Measured twice on `afr-kernel` with the S4 image loop-mounted `ro,nosuid,nodev`, the same bubblewrap namespace flags for both roots, and Landlock, seccomp and cgroups left out because they cost the same under either root. Each cell is the p50 range across the two runs:

| Measure | Image as root | Host binds |
|---|---|---|
| Sandbox start (`bwrap` → `true`, 100 runs each) | 1.89–3.35 ms | 2.01–3.35 ms |
| `git --version`, warm cache | 1.71–1.80 ms | 1.85–1.93 ms |
| `python3 -c pass`, warm cache | 6.72–6.81 ms | 6.38–6.53 ms |
| `git --version`, cold cache | 8.79–14.89 ms | 4.31–5.09 ms |
| `python3 -c pass`, cold cache | 16.70–17.46 ms | 10.22–11.10 ms |
| `node -e 0`, warm / cold | 44 / 83–84 ms | Node is not on the host |

Warm, the two roots match. Cold, the image costs about 6 ms more per binary, which is LZ4 decompression on first read; every sandbox on a host then shares the decompressed pages through the one mount (`docs/architecture/runner_execution.md` §Toolbox). The cold runs dropped every cache, so they are the worst case: admission hashes the whole image before the first lease. The image's git and Python are newer than the host's (2.47.3 against 2.43.0, 3.13.5 against 3.12.3).
