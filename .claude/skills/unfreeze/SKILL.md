---
name: unfreeze
version: 1.0.0
description: |
  Remove the directory freeze restriction set by /freeze or /guard.
  Edits are unrestricted again after running this. Use when asked to "unfreeze",
  "remove freeze", "unlock edits", or "done debugging".
allowed-tools:
  - Bash
---

# /unfreeze — Remove Edit Restriction

Clears the freeze boundary set by `/freeze` or `/guard`.

```bash
STATE_DIR="${CLAUDE_PLUGIN_DATA:-$HOME/.gstack}"
rm -f "$STATE_DIR/freeze-dir.txt"
echo "Freeze removed. Edits are unrestricted."
```

After running, confirm to the user: "Edit restriction removed. All files are now editable."
