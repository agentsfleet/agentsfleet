# Toolbox spikes for M211_001 : Oct 04, 2026

M211_001 Discovery requires six spikes before EXECUTE. This page holds their raw results; the spec keeps one line per spike and links here. Every spike ran on `feat/m211-sandbox-tools-and-nested-loops` at `3c8a5d2eb`, through a temporary kernel-lane trial that was never committed. The trial runs one script inside one lease with `Limits::default()`, under the production bubblewrap flags, Landlock and seccomp.

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

## S2 — dm-verity beneath EROFS: pending

So far: `/sys/module/dm_verity` is absent, and the kernel exposes no `/proc/config.gz`, so dm-verity is either built in or missing. `veritysetup` 2.7.0 is installed.

## S3 — Firecracker boots the exact image: not run

`afr-kernel` has no `/dev/kvm` (`ls /dev/kvm`: No such file or directory), and Firecracker needs it. S3 needs a bare-metal host or a cloud virtual machine with nested virtualization.

## S4 — unprivileged build: pending

## S5 — sandbox boundary: pending

## S6 — writable-state exhaustion: pending
