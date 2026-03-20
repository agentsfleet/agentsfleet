---
name: office-hours
version: 1.0.0
description: |
  YC Office Hours — two modes. Startup mode: six forcing questions that expose
  demand reality, status quo, narrowest wedge, and future-fit. Builder mode:
  design thinking for side projects, features, and open source. Saves a design doc.
  Use when asked to "brainstorm", "I have an idea", "help me think through this",
  "office hours", or "is this worth building".
  Proactively suggest when the user describes a new product idea or new milestone
  spec — before any code is written. Use before /plan-ceo-review or /plan-eng-review.
allowed-tools:
  - Bash
  - Read
  - Grep
  - Glob
  - Write
  - Edit
  - AskUserQuestion
---

# /office-hours — Idea Validation

Two modes. Ask the user which applies:

**A) Startup mode** — validating whether this idea/feature has real demand
**B) Builder mode** — designing a solution for a known problem

---

## Startup Mode — Six Forcing Questions

Work through these in order. One AskUserQuestion per question. Do not batch.

**1. Demand reality**
> Who specifically has asked for this, by name or role? How many times? What exactly did they say?

Push for concrete examples. "Users want X" is not an answer — "Three customers in our last three calls asked for X because Y" is.

**2. Status quo**
> What do they do today without this? How painful is it, concretely?

If the workaround is fine, the problem isn't real enough.

**3. Desperate specificity**
> Who is so desperate for this that they'd use a half-finished version today?

The answer reveals your actual ICP. If no one would use a rough version, the demand is hypothetical.

**4. Narrowest wedge**
> What is the smallest version that delivers real value to one user?

Not MVP — MLP (minimum lovable product). What's the one thing that makes someone say "finally"?

**5. Observation**
> Have you watched someone struggle with this problem in real time?

Described pain ≠ observed pain. If you haven't seen it happen, you're guessing.

**6. Future fit**
> In 12 months, does solving this make the product stronger or is it a detour?

Does this compound or distract?

After all six: summarize the signal. Is this a **lake** (real, boilable, worth building) or an **ocean** (vague, hypothetical, not yet)?

---

## Builder Mode — Design Thinking

For a known problem with a known user. Work through:

1. **Frame the problem** — one sentence: "When [user] tries to [goal], they struggle with [obstacle]."
2. **Constraints** — what are the hard limits? (time, stack, scope, external dependencies)
3. **Three approaches** — generate at least three distinct solutions before evaluating any.
4. **Tradeoffs** — for each: what do you gain, what do you give up, what can go wrong?
5. **Recommendation** — which approach and why, given the constraints.
6. **First action** — what's the first concrete step? (not "think more" — something you can do in the next hour)

---

## Output

Save the session as a design doc:

```bash
DATE=$(date +%Y-%m-%d)
SLUG=$(echo "<idea-name>" | tr '[:upper:]' '[:lower:]' | tr ' ' '-' | tr -cd '[:alnum:]-')
mkdir -p docs/design
cat > "docs/design/${DATE}-${SLUG}.md" << 'EOF'
# <Idea Name>

Date: <date>
Mode: Startup | Builder

## Problem
<one sentence>

## Key findings
<bullet points from the six questions or design session>

## Recommendation
<go / no-go / narrow further — with rationale>

## First action
<concrete next step>
EOF
echo "Design doc saved: docs/design/${DATE}-${SLUG}.md"
```

After saving, suggest next step:
- **Go** → run `/plan-ceo-review` to stress-test the scope
- **No-go** → note why in the doc, park it
- **Narrow further** → repeat with a smaller problem frame
