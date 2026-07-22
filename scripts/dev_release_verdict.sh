#!/usr/bin/env bash
# Compose the development deploy notification verdict from release-critical
# job results. Green is earned: quality (qa-dev), browser acceptance
# (acceptance-e2e-dev), and CLI acceptance (cli-acceptance-dev) must all be
# "success", and the worker deploy must be "success" or a documented "skipped"
# (DEV_WORKER_READY=false). Any failed, skipped, or cancelled release-critical
# result is red — a skip is not a pass.
#
# Inputs (env):  QA_RESULT ACCEPTANCE_RESULT CLI_RESULT WORKER_RESULT
#                REF_NAME SHORT_SHA RUN_URL
# Outputs:       verdict= color= message= appended to $GITHUB_OUTPUT
#                (stdout when unset), one dev_release_acceptance_summary
#                logfmt line per job on stderr. No secrets read or printed.
set -euo pipefail

QA="${QA_RESULT:?QA_RESULT is required}"
ACC="${ACCEPTANCE_RESULT:?ACCEPTANCE_RESULT is required}"
CLI="${CLI_RESULT:?CLI_RESULT is required}"
WORKER="${WORKER_RESULT:?WORKER_RESULT is required}"
REF="${REF_NAME:-main}"
SHORT="${SHORT_SHA:-unknown}"
RUN_URL="${RUN_URL:-}"

SUCCESS="success"
SKIPPED="skipped"
GREEN_COLOR=3066993
RED_COLOR=15158332

for entry in "qa-dev=$QA" "acceptance-e2e-dev=$ACC" "cli-acceptance-dev=$CLI" "deploy-worker-dev=$WORKER"; do
  echo "dev_release_acceptance_summary job=${entry%%=*} result=${entry#*=} commit=$SHORT" >&2
done

if [ "$QA" = "$SUCCESS" ] && [ "$ACC" = "$SUCCESS" ] && [ "$CLI" = "$SUCCESS" ] \
  && { [ "$WORKER" = "$SUCCESS" ] || [ "$WORKER" = "$SKIPPED" ]; }; then
  VERDICT="green"
  COLOR="$GREEN_COLOR"
  MESSAGE="✅ DEV deploy green — \`$REF\` @ \`$SHORT\`\\nQA: passed | acceptance-e2e: passed | cli-acceptance: passed | worker: ${WORKER}\\n→ ready for tag release"
else
  VERDICT="red"
  COLOR="$RED_COLOR"
  MESSAGE="❌ DEV deploy not releasable — \`$REF\` @ \`$SHORT\`\\nQA: ${QA} | acceptance-e2e: ${ACC} | cli-acceptance: ${CLI} | worker: ${WORKER}"
  if [ -n "$RUN_URL" ]; then
    MESSAGE="${MESSAGE}\\nCheck: ${RUN_URL}"
  fi
fi

write_outputs() {
  echo "verdict=$VERDICT"
  echo "color=$COLOR"
  echo "message=$MESSAGE"
}

if [ -n "${GITHUB_OUTPUT:-}" ]; then
  write_outputs >>"$GITHUB_OUTPUT"
else
  write_outputs
fi
