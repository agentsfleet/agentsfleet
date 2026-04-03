# M23_002: Homebrew Tap Setup

Establishes `usezombie/homebrew-tap` and wires it into the release pipeline so every `git push v*` tag auto-updates the formula.

Install command (end user):
```bash
brew install usezombie/tap/zombiectl
```

---

## Prerequisites

- M23_001 complete: at least one GitHub Release exists with `zombied-darwin-arm64.tar.gz` attached
- `gh` CLI authenticated to `usezombie` org
- `op` CLI authenticated with access to `ZMB_CD_PROD` vault

---

## Human Steps (do these once, then hand off)

### H1 — Create the tap repository

```bash
gh repo create usezombie/homebrew-tap \
  --public \
  --description "Homebrew tap for UseZombie CLI" \
  --gitignore "" \
  --license MIT
```

Verify:
```bash
gh repo view usezombie/homebrew-tap
```

### H2 — Add a GitHub App token for tap updates to vault

The `release.yml` tap-update job needs write access to `usezombie/homebrew-tap`. Use the existing GitHub App (already in vault) rather than a PAT.

Verify the app already has access:
```bash
op read "op://ZMB_CD_PROD/github-app/app-id"
op read "op://ZMB_CD_PROD/github-app/private-key" | head -1
```

If the existing GitHub App does **not** have access to `usezombie/homebrew-tap`, install it via the GitHub App settings page: `github.com/apps/<app-name>/installations`. Then proceed — no new vault items needed.

### H3 — Test brew install locally (after agent steps complete)

```bash
brew tap usezombie/tap
brew install usezombie/tap/zombiectl
zombiectl --version
brew test zombiectl
```

Expected: `zombiectl --version` prints the release version string, exits 0.

---

## Agent Steps (automated, no human interaction needed)

### A1 — Bootstrap the tap repo with the initial formula

Run after H1 completes. The agent writes `Formula/zombiectl.rb` to the tap repo.

```bash
# Fetch the latest release version and tarball SHA256
LATEST_TAG=$(gh api repos/usezombie/usezombie/releases/latest --jq '.tag_name')
VERSION="${LATEST_TAG#v}"
TARBALL_URL="https://github.com/usezombie/usezombie/releases/download/${LATEST_TAG}/zombied-darwin-arm64.tar.gz"
SHA256=$(curl -fsSL "$TARBALL_URL" | sha256sum | awk '{print $1}')

# Write the formula
cat > /tmp/zombiectl.rb <<EOF
class Zombiectl < Formula
  desc "UseZombie CLI — submit a spec, get a validated PR"
  homepage "https://usezombie.com"
  url "${TARBALL_URL}"
  sha256 "${SHA256}"
  version "${VERSION}"
  license "MIT"

  def install
    bin.install "zombied-darwin-arm64" => "zombied"
    # zombiectl is the user-facing binary name; delegate to zombied
    (bin/"zombiectl").write_env_script bin/"zombied", {}
  end

  test do
    assert_match version.to_s, shell_output("#{bin}/zombiectl --version")
  end
end
EOF

# Commit to the tap repo
gh api repos/usezombie/homebrew-tap/contents/Formula/zombiectl.rb \
  -f message="chore: add zombiectl formula v${VERSION}" \
  -f content="$(base64 < /tmp/zombiectl.rb)"
```

Verify:
```bash
gh api repos/usezombie/homebrew-tap/contents/Formula/zombiectl.rb --jq '.name'
# => zombiectl.rb
```

### A2 — Add `homebrew-tap` job to `release.yml`

Add the following job to `.github/workflows/release.yml` after the `create-release` job:

```yaml
  # ── homebrew-tap: auto-bump formula on every release ─────────────────────
  homebrew-tap:
    runs-on: ubuntu-latest
    needs: create-release
    permissions:
      contents: read
    steps:
      - uses: actions/checkout@v6

      - name: Load GitHub App credentials from 1Password
        uses: 1password/load-secrets-action@v4
        with:
          export-env: true
        env:
          OP_SERVICE_ACCOUNT_TOKEN: ${{ secrets.OP_SERVICE_ACCOUNT_TOKEN }}
          GITHUB_APP_ID: op://${{ vars.VAULT_PROD }}/github-app/app-id
          GITHUB_APP_PRIVATE_KEY: op://${{ vars.VAULT_PROD }}/github-app/private-key

      - name: Generate GitHub App token
        id: app-token
        uses: actions/create-github-app-token@v2
        with:
          app-id: ${{ env.GITHUB_APP_ID }}
          private-key: ${{ env.GITHUB_APP_PRIVATE_KEY }}
          owner: usezombie
          repositories: homebrew-tap

      - name: Bump formula in tap
        env:
          GH_TOKEN: ${{ steps.app-token.outputs.token }}
        run: |
          VERSION="${GITHUB_REF#refs/tags/v}"
          TARBALL_URL="https://github.com/usezombie/usezombie/releases/download/v${VERSION}/zombied-darwin-arm64.tar.gz"
          SHA256=$(curl -fsSL "$TARBALL_URL" | sha256sum | awk '{print $1}')

          # Fetch current formula blob SHA (required for update)
          BLOB_SHA=$(gh api repos/usezombie/homebrew-tap/contents/Formula/zombiectl.rb --jq '.sha')

          # Patch formula: update url, sha256, version lines
          FORMULA=$(gh api repos/usezombie/homebrew-tap/contents/Formula/zombiectl.rb --jq '.content' | base64 -d)
          UPDATED=$(echo "$FORMULA" \
            | sed "s|url \".*\"|url \"${TARBALL_URL}\"|" \
            | sed "s|sha256 \".*\"|sha256 \"${SHA256}\"|" \
            | sed "s|version \".*\"|version \"${VERSION}\"|")

          gh api repos/usezombie/homebrew-tap/contents/Formula/zombiectl.rb \
            -X PUT \
            -f message="chore: bump zombiectl to v${VERSION}" \
            -f content="$(echo "$UPDATED" | base64 -w0)" \
            -f sha="$BLOB_SHA"

          echo "✓ tap formula bumped to v${VERSION} (sha256: ${SHA256})"
```

Verify after adding the job:
```bash
make lint  # ensures no CI YAML syntax issues
```

### A3 — Verify the tap formula audit

```bash
brew tap usezombie/tap
brew audit --new-formula zombiectl
```

Expected: zero errors. Warnings about GitHub source URL are acceptable for a private tap.

---

## Rollback

If the formula is broken and `brew install` fails:

```bash
# Push a corrected formula directly
gh api repos/usezombie/homebrew-tap/contents/Formula/zombiectl.rb \
  -X PUT \
  -f message="fix: correct zombiectl formula" \
  -f content="$(base64 < /path/to/fixed-zombiectl.rb)" \
  -f sha="$(gh api repos/usezombie/homebrew-tap/contents/Formula/zombiectl.rb --jq '.sha')"
```

---

## Verify End-to-End

```bash
# macOS only
brew tap usezombie/tap
brew install usezombie/tap/zombiectl
zombiectl --version
brew test zombiectl
brew uninstall zombiectl
brew untap usezombie/tap
```
