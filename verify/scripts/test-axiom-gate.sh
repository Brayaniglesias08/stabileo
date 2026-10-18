#!/usr/bin/env bash
# Self-test of the axiom gate (scripts/check-axioms.sh): plant, one at a time, an `axiom`,
# an `@[implemented_by]` and an `@[extern]` in Certify/BundleParse.lean (the JSON reader,
# which only certcheck uses) and require
# * the exhaustive scan alone (`check-axioms.sh --scan-only`) to reject it, naming it, and
# * the full gate (whose source grep runs first) to reject it.
# The file is restored and rebuilt on exit. Run from verify/ after `lake build`.
set -euo pipefail
cd "$(dirname "$0")/.."

target=Certify/BundleParse.lean
backup=$(mktemp)
cp "$target" "$backup"
restore() {
	cp "$backup" "$target"
	rm -f "$backup"
	lake build +Certify.BundleParse >/dev/null
}
trap restore EXIT

plants=(
	'axiom tcbGateProbe : (1 : Nat) = 2'
	$'def tcbGateImpl (n : Nat) : Nat := n + 1\n@[implemented_by tcbGateImpl] def tcbGateProbe (n : Nat) : Nat := n'
	'@[extern "tcb_gate_probe"] opaque tcbGateProbe : Nat → Nat'
)
expects=('axiom declaration tcbGateProbe' '@[implemented_by] on tcbGateProbe' '@[extern] on tcbGateProbe')

for i in "${!plants[@]}"; do
	cp "$backup" "$target"
	printf '\n%s\n' "${plants[$i]}" >> "$target"
	if out=$(./scripts/check-axioms.sh --scan-only 2>&1); then
		echo "FAIL: the scan accepted: ${plants[$i]}" >&2
		exit 1
	fi
	if ! grep -qF "${expects[$i]}" <<<"$out"; then
		echo "FAIL: the scan rejected '${plants[$i]}' without reporting '${expects[$i]}':" >&2
		echo "$out" | tail -20 >&2
		exit 1
	fi
	if ./scripts/check-axioms.sh >/dev/null 2>&1; then
		echo "FAIL: the full gate accepted: ${plants[$i]}" >&2
		exit 1
	fi
	echo "ok: rejected ${expects[$i]#* on }${expects[$i]%% *} plant in $target"
done
echo "axiom gate self-test ok"
