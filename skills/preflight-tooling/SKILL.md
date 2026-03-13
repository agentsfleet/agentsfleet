---
name: preflight-tooling
description: Detect and install missing local developer tooling using mise-first and brew fallback before implementation work.
---

# Preflight Tooling

Detect missing tooling and install it before editing code.

## Policy

- Use `mise` first for language/runtime tooling.
- Use `brew` as fallback for non-runtime CLIs or when `mise` cannot install.
- Do not stop at detection only; install missing prerequisites when possible.
- Prefer `bun` to `node` when available.

## Required Tool Set

- Core: `git`, `gh`, `glab`, `tmux`
- Runtime/build: `mise`, `node`, `npm`, `bun`, `bunx`,`python`, `go`, `cargo`, `rustc`
- Containers: `docker` (Docker Desktop)
- Orchestration: `temporal`
- QA/automation: `playwright`, `stagehand`
- Ops/helpers: `oracle`, `imageoptim`, `trash`
- Optional: `zig`, `pass-cli`, `tailscale`, `zed`

## Command Workflow

```bash
# Runtime tools via mise first
mise use -g python@3.14
mise use -g node@latest
mise use -g bun@latest
mise use -g go@latest
mise use -g rust@stable
# CLI tools via brew fallback
brew install gh glab tmux imageoptim-cli trash

# Docker Desktop (brew cask)
brew install --cask docker

# Temporal CLI (mise first, brew fallback)
mise use -g aqua:temporalio/cli@latest
# fallback: brew install temporal

# Oracle CLI v0.9.2 (second-model review tool, @indykish/oracle)
npm install -g @indykish/oracle

# Optional (install only if needed)
# mise use -g zig@latest
# brew install tailscale
# brew install --cask zed
```

Then verify each required command with `command -v <tool>`.

Verification examples:

```bash
docker --version
temporal --version
playwright --version
stagehand --version
oracle --version
```

## E2E Networks Internal Setup

Only applies when working on repos hosted at `awakeninggit.e2enetworks.net`.
Skipped if sentinel `~/.config/e2e/.localdev_configured` exists.

```bash
[ -f ~/.config/e2e/.localdev_configured ] && echo "already configured" && exit 0

brew install openfortivpn mysql-client
# Optional: brew install --cask font-hack-nerd-font

# mysql-client PATH + flags
touch ~/.zshrc
if ! grep -q 'mysql-client/bin' ~/.zshrc; then
  cp ~/.zshrc ~/.zshrc.bak.$(date +%Y%m%d%H%M%S)
  MYSQL_PREFIX="$(brew --prefix mysql-client)"
  cat >> ~/.zshrc << EOF
export PATH="${MYSQL_PREFIX}/bin:\$PATH"
export LDFLAGS="-L${MYSQL_PREFIX}/lib"
export CPPFLAGS="-I${MYSQL_PREFIX}/include"
EOF
fi

# Ensure ~/.env_mac is sourced by ~/.zshrc
grep -q 'source.*\.env_mac' ~/.zshrc || echo '[ -f ~/.env_mac ] && source ~/.env_mac' >> ~/.zshrc

mkdir -p ~/.config/e2e && touch ~/.config/e2e/.localdev_configured
```

### VPN credentials

Add to `~/.env_mac` (sourced by `~/.zshrc`):

```bash
export VPN_PROD_HOST="<host>" VPN_PROD_USER="<user>" VPN_PROD_CERT="<hash>"
export VPN_STAGE_HOST="<host>" VPN_STAGE_USER="<user>" VPN_STAGE_CERT="<hash>"
```

### VPN launch scripts

```bash
mkdir -p ~/bin

cat > ~/bin/start-prod-vpn.sh << 'SCRIPT'
#!/bin/zsh
source ~/.env_mac 2>/dev/null
sudo openfortivpn "${VPN_PROD_HOST}:10443" -u "$VPN_PROD_USER" --trusted-cert "$VPN_PROD_CERT"
SCRIPT

cat > ~/bin/start-stage-vpn.sh << 'SCRIPT'
#!/bin/zsh
source ~/.env_mac 2>/dev/null
sudo openfortivpn "${VPN_STAGE_HOST}:10443" -u "$VPN_STAGE_USER" --trusted-cert "$VPN_STAGE_CERT"
SCRIPT

chmod +x ~/bin/start-prod-vpn.sh ~/bin/start-stage-vpn.sh

# Register as LaunchAgents (run once at login, silently fails if VPN unreachable)
mkdir -p ~/Library/LaunchAgents
for env in prod stage; do
  cat > ~/Library/LaunchAgents/com.user.start-${env}-vpn.plist << EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>Label</key>
  <string>com.user.start-${env}-vpn</string>
  <key>ProgramArguments</key>
  <array>
    <string>${HOME}/bin/start-${env}-vpn.sh</string>
  </array>
  <key>RunAtLoad</key>
  <true/>
</dict>
</plist>
EOF
done
```

## Rules

- If a package name differs from the command name, resolve it and continue.
- If installation fails, report exact command + error and the next fallback.
- Update `docs/tooling-inventory.md` when the baseline changes.
