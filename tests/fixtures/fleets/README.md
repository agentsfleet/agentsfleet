# Fixture fleets

The prime set: the fleets a newly primed environment, the acceptance suites and
the scenario pages all use. One file per fleet; the file says what the fleet is.

| Fleet | Journey | What it is |
|---|---|---|
| `morning-rooster` | first message | Answers its first message. |
| `careful-squirrel` | access | Asks before it touches a secret. |
| `night-owl` | wake-ups | Wakes on a schedule and a signed webhook. |
| `queen-bee`, `busy-bee` | fleet to fleet | One fleet directs another. |
| `stubborn-mule` | task | Keeps going until the job is done. |

Each file carries: `name` (the fleet's name), `journey`, `about`, `first_message`
(what a person types to create it), `follow_ups` (what they type next),
`remembers` (what it is told to pin), `asks_for` (access it requests),
`wakes_on` (schedules and triggers it adds), `steers` (fleets it directs) and
`proves` (what the journey shows end to end).

A journey that needs a unique value per run adds it to the message it sends;
the fleet's name and purpose stay as written here.
