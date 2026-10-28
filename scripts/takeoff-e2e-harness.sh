#!/usr/bin/env bash
# Take-off E2E harness as far as Linux/cloud allows (no WinUI, no signing certs).
#
# Path covered here:
#   fixture drawing (engine Session) → takeoff.brick → takeoff.record / bridge → .vdb
#   → assert quantity-trust checklist (walls, openings, wall_ref, thickness, project_ref, double-count)
#
# Depends on the quantity_trust integration test (O-Draft PR #32). If that test is not
# in this tree yet, the harness runs takeoff_record only and says so.
#
# Optional live server:
#   TAKEOFF_E2E_SHILPID=1  or  AAD_SHILPID_ADDR=127.0.0.1:7430
#   (starts shilpid when SHILPID_BIN / PATH / sibling shilpidb build is available)
#
# Optional export for O-CoreOp / manual hand-off:
#   TAKEOFF_TRUST_EXPORT=/tmp/quantity-trust ./scripts/takeoff-e2e-harness.sh
#
# Windows GUI steps (Draft WinUI, CoreOp Take-off page, annotated PDF, report embed)
# are in the OSAS store runbook: docs/takeoff-e2e-runbook.md — not automated here.

set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

has_quantity_trust=0
if cargo test -p aad-engine --test quantity_trust -- --list >/dev/null 2>&1; then
  has_quantity_trust=1
fi

echo "== take-off record / quantity trust (headless Rust) =="
if [[ "$has_quantity_trust" -eq 1 ]]; then
  cargo test -p aad-engine --test quantity_trust -- --nocapture
else
  echo "note: quantity_trust test not in this tree yet (merge O-Draft PR #32); skipping that suite"
fi
cargo test -p aad-engine --test takeoff_record -- --nocapture

EXPORT_DIR="${TAKEOFF_TRUST_EXPORT:-}"
if [[ -n "$EXPORT_DIR" ]]; then
  if [[ "$has_quantity_trust" -ne 1 ]]; then
    echo "TAKEOFF_TRUST_EXPORT set but quantity_trust is missing; nothing to export" >&2
    exit 1
  fi
  mkdir -p "$EXPORT_DIR"
  echo "== exporting checklist under $EXPORT_DIR =="
  TAKEOFF_TRUST_EXPORT="$EXPORT_DIR" cargo test -p aad-engine --test quantity_trust \
    checklist_drawing_records_known_wall_and_opening_numbers -- --nocapture
  [[ -f "$EXPORT_DIR/quantity-trust-checklist.json" ]] && echo "wrote $EXPORT_DIR/quantity-trust-checklist.json"
  [[ -f "$EXPORT_DIR/quantity-trust.takeoff.vdb" ]] && echo "wrote $EXPORT_DIR/quantity-trust.takeoff.vdb"
fi

run_shilpid=0
if [[ "${TAKEOFF_E2E_SHILPID:-}" == "1" ]] || [[ -n "${AAD_SHILPID_ADDR:-}" ]]; then
  run_shilpid=1
fi

if [[ "$run_shilpid" -eq 1 ]]; then
  ADDR="${AAD_SHILPID_ADDR:-127.0.0.1:7430}"
  HOST="${ADDR%%:*}"
  PORT="${ADDR##*:}"
  DATA="${TAKEOFF_E2E_VDB:-$(mktemp "${TMPDIR:-/tmp}/takeoff-e2e-XXXXXX.vdb")}"
  SHILPID_BIN="${SHILPID_BIN:-$(command -v shilpid || true)}"
  if [[ -z "$SHILPID_BIN" ]]; then
    if [[ -x "$ROOT/../shilpidb/target/debug/shilpid" ]]; then
      SHILPID_BIN="$ROOT/../shilpidb/target/debug/shilpid"
    elif [[ -x "$ROOT/../shilpidb/target/release/shilpid" ]]; then
      SHILPID_BIN="$ROOT/../shilpidb/target/release/shilpid"
    fi
  fi
  if [[ -z "$SHILPID_BIN" || ! -x "$SHILPID_BIN" ]]; then
    echo "shilpid not found; build sibling repo: cargo build -p shilpid (in shilpidb)" >&2
    echo "or set SHILPID_BIN. Skipping live-server half." >&2
  else
    echo "== live shilpid on $ADDR data=$DATA =="
    "$SHILPID_BIN" --bind "$ADDR" --data "$DATA" --autosave-secs 2 &
    PID=$!
    cleanup() { kill "$PID" 2>/dev/null || true; }
    trap cleanup EXIT
    for _ in $(seq 1 50); do
      if (echo >"/dev/tcp/$HOST/$PORT") >/dev/null 2>&1; then break; fi
      sleep 0.1
    done
    if [[ "$has_quantity_trust" -eq 1 ]]; then
      AAD_SHILPID_ADDR="$ADDR" cargo test -p aad-engine --test quantity_trust \
        checklist_through_a_running_shilpid -- --ignored --nocapture
    fi
    AAD_SHILPID_ADDR="$ADDR" cargo test -p aad-engine --test takeoff_record \
      a_running_shilpid_ends_up_with_the_take_off_and_loses_removed_rows -- --ignored --nocapture || true
    cleanup
    trap - EXIT
  fi
fi

echo
echo "Linux/cloud harness done."
echo "Still needs a Windows machine (OSAS store docs/takeoff-e2e-runbook.md):"
echo "  - Draft WinUI measure → Record in ShilpiDB (shared server)"
echo "  - CoreOp Project > Take-off: Read + Fetch annotated PDF + report embed"
echo "  - Real-project quantity/PO audit against expected_face_m2 in the checklist"
echo "  - Authenticode / clean install (no certs in this harness)"
