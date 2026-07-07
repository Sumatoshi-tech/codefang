#!/usr/bin/env bash
# Integrity gate entry point (SPEC §3.7/§3.8, roadmap 7). Runs, in order:
#   1. tamper_check.py --self-test  -- prove the tamper checker catches each class
#   2. tamper_check.py (verify)     -- live fail-closed check on the real harness
#   3. mutation_self_test.sh        -- META-GATE: prove the system catches a real
#                                      product bug AND a harness cheat
# Exit 0 only if ALL pass. This is the required, non-bypassable integrity gate.
set -u
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
rc=0

echo "########## 1/3 tamper-check SELF-TEST ##########"
python3 "$HERE/tamper_check.py" --self-test || rc=1

echo; echo "########## 2/3 tamper-check VERIFY (fail-closed) ##########"
python3 "$HERE/tamper_check.py" || rc=1

echo; echo "########## 3/3 MUTATION SELF-TEST (meta-gate) ##########"
bash "$HERE/mutation_self_test.sh" || rc=1

echo
if [ "$rc" -eq 0 ]; then
  echo "INTEGRITY GATE: GREEN -- harness untampered; system provably catches a"
  echo "product bug and fails closed on a harness cheat."
else
  echo "INTEGRITY GATE: RED -- see failures above. Do NOT trust a green compat run."
fi
exit $rc
