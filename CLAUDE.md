# Oracle Operating Model

This file is a thin pointer to the canonical [AGENTS.md](./AGENTS.md).

- Update policy and workflow guidance in `AGENTS.md` only.
- Keep this file in sync by updating the pointer if the path changes.
- Dotfile edits (`.*`) must be preceded by a timestamped backup; follow the rule in `AGENTS.md`.
- Team shorthand compatibility (legacy CTO/Senior Engineer language) is defined in `AGENTS.md` under `Legacy Team Lenses (AGENTS_OLD Compatibility)`.
- Date-time format standard for docs/notes: `Feb 02, 2026: 10:30 AM`.
- Sync is mandatory, not user-prompted: after any change under `~/Projects/ai-jumpstart/*` (except `README.md`), sync mapped files to `~/Projects/dotfiles` in the same turn and explicitly report `sync completed + verified`.
- Oracle review defaults (inline CTO/Engineer lens primary, `@indykish/oracle` CLI secondary) are defined in `AGENTS.md` under `Tool Commands (Primary)` and `skills/oracle/SKILL.md`.

## Skill routing

When the user's request matches an available skill, ALWAYS invoke it using the Skill
tool as your FIRST action. Do NOT answer directly, do NOT use other tools first.
The skill has specialized workflows that produce better results than ad-hoc answers.

Key routing rules:
- Product ideas, "is this worth building", brainstorming → invoke office-hours
- Bugs, errors, "why is this broken", 500 errors → invoke investigate
- Ship, deploy, push, create PR → invoke ship
- QA, test the site, find bugs → invoke qa
- Code review, check my diff → invoke review
- Update docs after shipping → invoke document-release
- Weekly retro → invoke retro
- Design system, brand → invoke design-consultation
- Visual audit, design polish → invoke design-review
- Architecture review → invoke plan-eng-review
- Save progress, checkpoint, resume → invoke checkpoint
- Code quality, health check → invoke health
