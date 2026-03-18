---
name: document-release
version: 1.0.0
description: |
  Generate release documentation from VERSION, CHANGELOG, and git history.
  Updates user-facing docs for the release.
allowed-tools:
  - Read
  - Write
  - Bash
---

# Document Release

Generate release notes and update user-facing documentation after a ship.

## Step 1: Gather release context

```bash
cat VERSION
git log --oneline -10
git log $(git describe --tags --abbrev=0 2>/dev/null || echo "HEAD~10")..HEAD --oneline
```

Read CHANGELOG.md to see the release entry.

## Step 2: Identify user-visible changes

From the commits and CHANGELOG, extract:

- **New features**: User-facing capabilities added
- **Breaking changes**: API changes requiring migration
- **Deprecations**: Features scheduled for removal
- **Bug fixes**: User-visible fixes (not internal refactors)
- **API changes**: New endpoints, changed parameters, removed endpoints

Skip internal changes: refactors, test improvements, CI changes.

## Step 3: Update docs

Update relevant documentation files:

1. **README.md**: Update version badge, new feature highlights
2. **API docs**: Document new/changed endpoints
3. **Migration guide**: If breaking changes, add migration notes
4. **Changelog**: Already updated by `/ship` — verify completeness

## Step 4: Write release notes

If a `RELEASE_NOTES.md` exists, append the new release section. Format:

```markdown
## [X.Y.Z] - YYYY-MM-DD

### Added
- Feature description

### Changed
- Change description

### Fixed
- Fix description

### Breaking
- Breaking change with migration path
```

## Step 5: Commit docs

```bash
git add docs/ README.md RELEASE_NOTES.md
git commit -m "docs: update for v<VERSION> release"
```

## Output

Report what was documented:
- Files updated
- Release summary (features, fixes, breaking changes)