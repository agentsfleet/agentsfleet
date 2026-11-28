# Fixture fleets

These fleets are the prime set. A newly primed environment and both acceptance suites use them.
Each file says what its fleet is and what running it proves.

| Fleet | Covers | What it is |
|---|---|---|
| `morning-rooster` | first message | Answers its first message. |
| `curious-otter` | public web | Searches for a primary source, reads the first result and cites it. |
| `keen-hawk` | image messages | Identifies the colour in an attached picture. |
| `careful-squirrel` | access | Asks before it touches a secret. |
| `night-owl` | wake-ups | Wakes on a schedule and a signed webhook. |
| `queen-bee`, `busy-bee` | fleet to fleet | One fleet directs another. |
| `stubborn-mule` | task | Keeps going until the job is done. |
| `watch-dog` | GitHub | Asks for one repository, and wakes when a person opens a pull request there. |
| `chatty-parrot` | Slack | Answers a mention in the thread it came from. |

## Fields

| Field | Holds |
|---|---|
| `name` | The fleet's name. |
| `journey` | What the fleet covers, as in the table above. |
| `about` | What the fleet is, in one or two sentences. |
| `first_message` | What a person types to create it. |
| `follow_ups` | What they type next. |
| `remembers` | What it is told to pin. |
| `asks_for` | The access it requests. |
| `wakes_on` | The schedules and triggers it adds. |
| `steers` | The fleets it directs. |
| `repository` | For `watch-dog`, the test repository the platform's GitHub App is installed on. |
| `image_png` | For `keen-hawk`, the base64-encoded test picture. |
| `proves` | What running it shows, end to end. |

A test that needs a unique value per run adds it to the message it sends.
The fleet's name and purpose stay as written here.
