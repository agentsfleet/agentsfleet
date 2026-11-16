#!/usr/bin/env bash
# Runs local acceptance (make acceptance-codex) on a throwaway full VM and
# brings the evidence back. Providers:
#   aws-spot  EC2 Spot, Debian 13 amd64; talks to AWS over HTTPS only (no SSH)
#   gh        the acceptance-on-demand workflow on a GitHub-hosted runner
# Setup and costs: docs/development.md, "Burst acceptance".
set -euo pipefail

HERE="$(cd "$(dirname "$0")" && pwd -P)"
readonly HERE
readonly WORKFLOW=acceptance-on-demand.yml
readonly BUN_VERSION=1.4.4
# x86 only: the signed toolbox the dev deploy publishes is amd64. Tried in
# order; a type without Spot capacity falls through to the next.
readonly DEFAULT_TYPES="c7i.4xlarge c7a.4xlarge c6i.4xlarge m7i.4xlarge"
readonly DEFAULT_IMAGE="resolve:ssm:/aws/service/debian/release/13/latest/amd64"

die() { printf 'acceptance-burst: %s\n' "$*" >&2; exit 1; }
usage() {
  cat >&2 <<'USAGE'
usage: acceptance-burst.sh [--provider aws-spot|gh] [--suite e2e|cli|all] [--ref <branch>] [--keep]
USAGE
  exit 2
}

provider=aws-spot suite=e2e ref="" keep=0
while [ $# -gt 0 ]; do
  case "$1" in
    --provider) provider="${2:-}"; shift 2 ;;
    --suite) suite="${2:-}"; shift 2 ;;
    --ref) ref="${2:-}"; shift 2 ;;
    --keep) keep=1; shift ;;
    -h | --help) usage ;;
    *) printf 'acceptance-burst: unknown argument %s\n' "$1" >&2; usage ;;
  esac
done
case "$suite" in e2e | cli | all) ;; *) die "--suite must be e2e, cli or all" ;; esac
ref="${ref:-$(git rev-parse --abbrev-ref HEAD)}"
root="$(git rev-parse --show-toplevel)"
poll="${ACCEPTANCE_BURST_POLL_SECONDS:-30}"
id="$(printf '%s' "$ref" | tr -c 'A-Za-z0-9-' '-' | cut -c1-40)-$(date -u +%Y%m%d%H%M%S)"
out="${ACCEPTANCE_BURST_OUT:-${TMPDIR:-/tmp}/acceptance-burst}/$id"
mkdir -p "$out"
started=$(date +%s)

summary() {
  local minutes=$(( ($(date +%s) - started + 59) / 60 ))
  printf 'acceptance-burst: provider=%s ref=%s suite=%s minutes=%s %s exit=%s evidence=%s\n' \
    "$provider" "$ref" "$suite" "$minutes" "$1" "$2" "$out"
}

run_gh() {
  command -v gh >/dev/null || die "gh is required"
  # A dispatch works once the workflow is on the default branch; before that,
  # label the pull request "acceptance" instead.
  gh workflow run "$WORKFLOW" -f ref="$ref" -f suite="$suite"
  local run="" attempt
  for attempt in 1 2 3 4 5 6 7 8 9 10; do
    run="$(gh run list --workflow "$WORKFLOW" --event workflow_dispatch --limit 1 \
      --json databaseId,createdAt --jq '.[0].databaseId')"
    [ -n "$run" ] && break
    sleep "$poll"
  done
  [ -n "$run" ] || die "the dispatched run did not appear (attempts: $attempt)"
  local status=0
  gh run watch "$run" --exit-status >/dev/null || status=$?
  gh run download "$run" --dir "$out" || true
  summary "run=$run cost=free-minutes" "$status"
  return "$status"
}

aws_() { aws --region "$region" "$@"; }

run_aws() {
  command -v aws >/dev/null || die "aws is required"
  region="${ACCEPTANCE_BURST_REGION:-us-east-1}"
  # Globals, not locals: the EXIT trap reads them after this function returns.
  bucket="${ACCEPTANCE_BURST_BUCKET:?set ACCEPTANCE_BURST_BUCKET (docs/development.md, Burst acceptance)}"
  local profile="${ACCEPTANCE_BURST_INSTANCE_PROFILE:-acceptance-burst}"
  local types="${ACCEPTANCE_BURST_INSTANCE_TYPES:-$DEFAULT_TYPES}"
  local image="${ACCEPTANCE_BURST_IMAGE:-$DEFAULT_IMAGE}"
  local vault="${ACCEPTANCE_BURST_VAULT:-ZMB_CD_DEV}"
  local parameter="${ACCEPTANCE_BURST_OP_PARAMETER:-/acceptance-burst/op-service-account-token}"
  local deadline="${ACCEPTANCE_BURST_DEADLINE_MINUTES:-120}"
  prefix="runs/$id" instance="" type=""

  cleanup() {
    if [ -n "$instance" ] && [ "$keep" = 0 ]; then
      aws_ ec2 terminate-instances --instance-ids "$instance" >/dev/null || true
    fi
    aws_ s3 rm --recursive --only-show-errors "s3://$bucket/$prefix" || true
  }
  trap cleanup EXIT

  echo "acceptance-burst: staging $ref in s3://$bucket/$prefix"
  git -C "$root" bundle create "$out/repo.bundle" "$ref" 2>/dev/null
  aws_ s3 cp --only-show-errors "$out/repo.bundle" "s3://$bucket/$prefix/repo.bundle"
  local toolbox="${ACCEPTANCE_BURST_TOOLBOX_DIR:-}"
  if [ -z "$toolbox" ]; then
    command -v gh >/dev/null || die "set ACCEPTANCE_BURST_TOOLBOX_DIR or install gh to fetch the signed toolbox"
    local deploy
    deploy="$(gh run list --workflow deploy-dev.yml --branch main --status success --limit 1 --json databaseId --jq '.[0].databaseId')"
    [ -n "$deploy" ] || die "no successful deploy-dev run on main to take a signed toolbox from"
    toolbox="$out/toolbox"
    gh run download "$deploy" --name runner-toolbox --dir "$toolbox" || die "the deploy's signed toolbox has expired; run deploy-dev or set ACCEPTANCE_BURST_TOOLBOX_DIR"
  fi
  aws_ s3 cp --recursive --only-show-errors "$toolbox" "s3://$bucket/$prefix/toolbox"

  {
    echo '#!/bin/bash'
    printf 'BURST_%s=%q\n' BUCKET "$bucket" PREFIX "$prefix" REF "$ref" SUITE "$suite" REGION "$region" \
      VAULT "$vault" OP_PARAMETER "$parameter" DEADLINE_MINUTES "$deadline" BUN_VERSION "$BUN_VERSION"
    tail -n +2 "$HERE/acceptance-burst/aws-user-data.sh"
  } >"$out/user-data.sh"

  local subnet=()
  [ -n "${ACCEPTANCE_BURST_SUBNET:-}" ] && subnet=(--subnet-id "$ACCEPTANCE_BURST_SUBNET")
  for type in $types; do
    if instance="$(aws_ ec2 run-instances --image-id "$image" --instance-type "$type" ${subnet[@]+"${subnet[@]}"} \
      --instance-market-options 'MarketType=spot,SpotOptions={SpotInstanceType=one-time,InstanceInterruptionBehavior=terminate}' \
      --instance-initiated-shutdown-behavior terminate \
      --iam-instance-profile "Name=$profile" \
      --metadata-options HttpTokens=required \
      --block-device-mappings 'DeviceName=/dev/xvda,Ebs={VolumeSize=80,VolumeType=gp3,DeleteOnTermination=true}' \
      --tag-specifications "ResourceType=instance,Tags=[{Key=owner,Value=agent},{Key=acceptance-burst,Value=$id}]" \
      --user-data "file://$out/user-data.sh" \
      --query 'Instances[0].InstanceId' --output text 2>"$out/launch.err")"; then
      break
    fi
    instance=""
    grep -qE 'InsufficientInstanceCapacity|SpotMaxPriceTooLow|MaxSpotInstanceCountExceeded|Unsupported' "$out/launch.err" \
      || die "run-instances refused: $(cat "$out/launch.err")"
    echo "acceptance-burst: no Spot capacity for $type; trying the next type"
  done
  [ -n "$instance" ] || die "no Spot capacity for any of: $types"
  echo "acceptance-burst: $instance ($type) running; streaming its log"

  local shown=0 status="" limit=$(( started + (deadline + 15) * 60 ))
  while [ -z "$status" ]; do
    sleep "$poll"
    if aws_ s3 cp --only-show-errors "s3://$bucket/$prefix/run.log" "$out/run.log" 2>/dev/null; then
      local lines
      lines=$(wc -l <"$out/run.log" | tr -d ' ')
      [ "$lines" -gt "$shown" ] && tail -n +"$((shown + 1))" "$out/run.log" && shown=$lines
    fi
    status="$(aws_ s3 cp "s3://$bucket/$prefix/exit-code" - 2>/dev/null | tr -d '[:space:]' || true)"
    if [ -z "$status" ]; then
      local state
      state="$(aws_ ec2 describe-instances --instance-ids "$instance" \
        --query 'Reservations[0].Instances[0].State.Name' --output text 2>/dev/null || true)"
      case "$state" in shutting-down | terminated) status=interrupted ;; esac
      [ "$(date +%s)" -lt "$limit" ] || status=deadline
    fi
  done

  aws_ s3 cp --only-show-errors "s3://$bucket/$prefix/results.tar.gz" "$out/results.tar.gz" 2>/dev/null \
    && tar -xzf "$out/results.tar.gz" -C "$out"
  local price
  price="$(aws_ ec2 describe-spot-price-history --instance-types "$type" --product-descriptions Linux/UNIX \
    --max-items 1 --query 'SpotPriceHistory[0].SpotPrice' --output text 2>/dev/null || echo unknown)"
  case "$status" in
    interrupted) summary "type=$type spot=\$$price/h" interrupted; die "the instance stopped before reporting (Spot interruption?)" ;;
    deadline) summary "type=$type spot=\$$price/h" deadline; die "no result within ${deadline}+15 minutes" ;;
  esac
  summary "type=$type spot=\$$price/h" "$status"
  return "$status"
}

case "$provider" in
  aws-spot) run_aws ;;
  gh) run_gh ;;
  *) die "--provider must be aws-spot or gh" ;;
esac
