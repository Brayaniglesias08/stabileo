#!/usr/bin/env bash
# Self-test of the axiom gate (scripts/check-axioms.sh): plant, one at a time, an `axiom`,
# an `@[implemented_by]` and an `@[extern]` in Certify/BundleParse.lean (the JSON reader,
# which only certcheck uses) and require
# * the exhaustive scan alone (`check-axioms.sh --scan-only`) to reject it, naming it, and
# * the full gate (whose source grep runs first) to reject it.
#
# The probes never touch this checkout: everything happens in a temporary copy of verify/
# (sources copied, Mathlib and the other packages linked, the Certify build outputs copied
# so only BundleParse and CertCheck rebuild), deleted on exit. Run from verify/ after
# `lake build` and `scripts/check-axioms.sh` (which builds CertCheck).
set -euo pipefail
cd "$(dirname "$0")/.."
src=$(pwd)

work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
copy=$work/verify
mkdir -p "$copy/.lake"
# Everything but .lake; then the packages by link and the library build by copy.
tar -C "$src" --exclude ./.lake -cf - . | tar -C "$copy" -xf -
ln -s "$src/.lake/packages" "$copy/.lake/packages"
if [ -d "$src/.lake/build" ]; then
	cp -R "$src/.lake/build" "$copy/.lake/build"
fi
target=$copy/Certify/BundleParse.lean
pristine=$work/BundleParse.lean
cp "$target" "$pristine"

plants=(
	'axiom tcbGateProbe : (1 : Nat) = 2'
	$'def tcbGateImpl (n : Nat) : Nat := n + 1\n@[implemented_by tcbGateImpl] def tcbGateProbe (n : Nat) : Nat := n'
	'@[extern "tcb_gate_probe"] opaque tcbGateProbe : Nat → Nat'
)
expects=('axiom declaration tcbGateProbe' '@[implemented_by] on tcbGateProbe' '@[extern] on tcbGateProbe')

for i in "${!plants[@]}"; do
	cp "$pristine" "$target"
	printf '\n%s\n' "${plants[$i]}" >> "$target"
	if out=$("$copy/scripts/check-axioms.sh" --scan-only 2>&1); then
		echo "FAIL: the scan accepted: ${plants[$i]}" >&2
		exit 1
	fi
	if ! grep -qF "${expects[$i]}" <<<"$out"; then
		echo "FAIL: the scan rejected '${plants[$i]}' without reporting '${expects[$i]}':" >&2
		echo "$out" | tail -20 >&2
		exit 1
	fi
	if out=$("$copy/scripts/check-axioms.sh" 2>&1); then
		echo "FAIL: the full gate accepted: ${plants[$i]}" >&2
		exit 1
	fi
	if ! grep -q 'axiom / implemented_by / extern declaration found' <<<"$out"; then
		echo "FAIL: the full gate's source grep missed: ${plants[$i]}" >&2
		echo "$out" | tail -20 >&2
		exit 1
	fi
	echo "ok: ${expects[$i]} rejected by the scan and by the source grep"
done
# The checkout itself is untouched and still passes.
git -C "$src" diff --quiet -- Certify || { echo "FAIL: the checkout's Certify/ changed" >&2; exit 1; }
echo "axiom gate self-test ok"
