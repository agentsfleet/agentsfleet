---
name: update-docs-and-commit
description: Update user-visible documentation based on code changes, then prepare clean, focused commits when requested.
---

# Update Docs and Commit

Update documentation based on code changes, then commit everything.

## Instructions

Be conservative — only update docs for **user-visible changes**.

## Process

1. **Analyze git changes**
   - Run `git status`, `git diff`, and `git diff --staged`
   - Identify user-facing changes vs internal changes

2. **Update CHANGELOG.md if needed**

   **Update for:**
   - New features, bug fixes, breaking changes
   - Deprecated APIs, removed features
   - Performance improvements users will notice

   **Skip for:**
   - Internal refactoring
   - Documentation-only updates
   - Code cleanup, formatting
   - Test changes

3. **CHANGELOG format**

   Add entry under `## [Unreleased]` section:
   ```markdown
   ## [Unreleased]

   ### Added
   - New feature description

   ### Changed
   - Changed behavior description

   ### Fixed
   - Bug fix description
   ```

4. **Check other docs**

   If behavior, APIs, or operator steps changed, also check:
   - `README.md` — usage instructions, examples, install steps
   - `docs/*.md` — any relevant doc files
   - Inline help text or CLI `--help` output

   Only update what's stale. Don't rewrite docs that are still accurate.

5. **Version bumping**

   **DO NOT** auto-bump version numbers unless the user explicitly requests it.
   - Ask the user which version (MAJOR.MINOR.PATCH) when they request a bump.
   - When bumping, rename `## [Unreleased]` → `## [X.Y.Z] - YYYY-MM-DD` in CHANGELOG.md.
   - Add a fresh empty `## [Unreleased]` section above the new version.
   - Update the comparison links at the bottom of CHANGELOG.md.

6. **Detect forge before committing**

   Run `git remote -v` and pick the correct CLI:

   | Remote host contains | Use |
   |---|---|
   | `awakeninggit.e2enetworks.net` | `glab` |
   | `gitlab.com` | `glab` |
   | `github.com` | `gh` |

   Use the detected tool for any PR/MR or CI operations in later steps.

7. **Stage and commit**
   - Stage all related files (code changes + updated docs)
   - Commit message: `docs: update CHANGELOG for [brief description]`
   - If code changes are also staged: use a descriptive Conventional Commit instead (e.g. `feat:`, `fix:`)

8. **Push and create MR/PR**

   Protected branches (e.g. `main`) block direct push. Always push via a branch.

   ```bash
   # Create branch and push
   git checkout -b <type>/<short-description>
   git push -u origin <type>/<short-description>
   ```

   Then create the MR/PR using the detected forge tool:

   **GitLab (`glab`):**
   ```bash
   glab mr create \
     --title "<type>: <description>" \
     --description "## Summary
   - Change 1
   - Change 2

   ## Test plan
   - [ ] Verify step" \
     --target-branch main \
     --no-editor
   ```

   **GitHub (`gh`):**
   ```bash
   gh pr create \
     --title "<type>: <description>" \
     --body "## Summary
   - Change 1
   - Change 2

   ## Test plan
   - [ ] Verify step" \
     --base main
   ```

   If the forge CLI is not authenticated for the remote host, print the
   MR/PR creation URL from the `git push` output and tell the user to
   authenticate. If `pass-cli` is unavailable, skip MR creation and stop
   after printing the URL.
   ```bash
   # GitLab self-hosted (SSH remotes)
   glab auth login --hostname <host>

   # GitLab with pass-cli (Proton Pass)
   TOKEN="$(pass-cli item view --vault-name "AGENTS_BUFFET" --item-title "GITLAB_PERSONAL_ACCESS_TOKEN" --field password)"
   glab auth login --hostname <host> --token "$TOKEN"

   # GitHub
   gh auth login
   ```

9. **Verify**
   - Confirm commit succeeded with `git log --oneline -1`
   - Confirm no unstaged changes remain with `git status`
   - Confirm MR/PR was created or URL was provided

## Output

1. Analysis of changes (user-visible or internal?)
2. Docs updated (CHANGELOG, README, or others — or "No updates needed")
3. Commit created, branch pushed, and MR/PR created (or URL provided)
