# Committed baselines

One file per lane per profile, each a result a lane actually produced on the
rig. A comparison prints how a later run moved against the file beside it and
exits zero whichever direction that was — nothing here gates a build.

**These are reference points, not targets.** They were measured on one
developer machine against compose datastores, so the absolute numbers say what
one process reached on that hardware, and a run on different hardware will
differ for reasons that have nothing to do with the code. What survives the
move is the SHAPE: how many Postgres round trips a lease costs, whether an idle
poll touches Postgres at all, and how much of a window a runner fleet spends
finding nothing.

Replacing a baseline is a deliberate act. Copy the result over it in the same
commit as the change that moved it, and say in the commit message which number
moved and why — a baseline updated silently is a regression nobody saw.

No baseline exists for the `prod` profile and none should: that profile is
built as a refusal and never runs.
