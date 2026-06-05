#!/bin/sh
# Voice stub for tests.
# Env vars:
#   VOICE_STUB_EXIT       exit code (default 0)
#   VOICE_STUB_EVENTS     number of progress events to emit (default 1)
#   VOICE_STUB_SLEEP      seconds to sleep before exiting (default 0)
#   VOICE_STUB_QUESTIONS  question text for exit 4 (needs-input)
#   VOICE_STUB_VERDICT    "pass" or "fail" - adds verdict object to report
#   VOICE_STUB_FINDINGS   finding detail for verdict.passed=false

write_report() {
  reason="$1"

  questions_json="[]"
  if [ -n "${VOICE_STUB_QUESTIONS:-}" ] && [ "$reason" = "needs-input" ]; then
    questions_json='[{"id":"q1","prompt":"'"${VOICE_STUB_QUESTIONS}"'","kind":"question"}]'
  fi

  verdict_json="null"
  if [ "${VOICE_STUB_VERDICT:-}" = "pass" ]; then
    verdict_json='{"passed":true,"findings":[]}'
  elif [ "${VOICE_STUB_VERDICT:-}" = "fail" ]; then
    findings_detail="${VOICE_STUB_FINDINGS:-verifier found issues}"
    verdict_json='{"passed":false,"findings":[{"severity":"blocker","detail":"'"${findings_detail}"'"}]}'
  fi

  mkdir -p "$(dirname "$VOICE_REPORT_PATH")"
  printf '{
  "schema": "score.run-report/v1",
  "run_id": "%s",
  "ticket_id": "%s",
  "role": "builder",
  "model": {"provider": "stub", "id": "stub"},
  "exit_reason": "%s",
  "questions": %s,
  "verdict": %s,
  "started_at": "2026-06-05T00:00:00Z",
  "finished_at": "2026-06-05T00:00:01Z",
  "env": {
    "VOICE_TICKET_PATH": "%s",
    "VOICE_WORKSPACE": "%s",
    "VOICE_ROLE_MANIFEST": "%s",
    "VOICE_REPORT_PATH": "%s",
    "VOICE_RUN_ID": "%s"
  }
}\n' \
    "$VOICE_RUN_ID" \
    "$(basename "$VOICE_TICKET_PATH" .yaml)" \
    "$reason" \
    "$questions_json" \
    "$verdict_json" \
    "$VOICE_TICKET_PATH" \
    "$VOICE_WORKSPACE" \
    "$VOICE_ROLE_MANIFEST" \
    "$VOICE_REPORT_PATH" \
    "$VOICE_RUN_ID" \
    > "$VOICE_REPORT_PATH"
}

trap 'write_report cancelled; exit 5' TERM

count="${VOICE_STUB_EVENTS:-1}"
i=1
while [ "$i" -le "$count" ]; do
  printf '{"schema":"score.voice-event/v1","run_id":"%s","t":"status","msg":"event-%s"}\n' \
    "$VOICE_RUN_ID" "$i"
  i=$((i + 1))
done

if [ "${VOICE_STUB_SLEEP:-0}" != "0" ]; then
  sleep "$VOICE_STUB_SLEEP"
fi

case "${VOICE_STUB_EXIT:-0}" in
  0) write_report completed ;;
  3) write_report infeasible ;;
  4) write_report needs-input ;;
  5) write_report cancelled ;;
  *) write_report failed ;;
esac

exit "${VOICE_STUB_EXIT:-0}"
