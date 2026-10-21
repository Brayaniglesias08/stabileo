#!/usr/bin/env bash
# Self-test of the axiom gate (scripts/check-axioms.sh): plant, one at a time, an `axiom`,
# an `@[implemented_by]` and an `@[extern]` in Certify/BundleParse.lean (the JSON reader,
# which only certcheck uses) and require
# * the exhaustive scan alone (`check-axioms.sh --scan-only`) to reject it, naming it, and
# * the full gate to reject it in its source grep (which runs before any Lean).
#
# The probes never touch this checkout: everything happens in a temporary copy of verify/
# (sources copied, the packages linked, the Certify build outputs copied so only
# BundleParse and CertCheck rebuild), deleted on exit. The checkout's sources are hashed at
# entry and must hash the same at the end (uncommitted edits are fine).
#
# Then the self-test checks itself: each of its assertions must fail against a broken gate
# (one that accepts everything, one that rejects without naming the plant, one whose grep
# misses it), and the checkout hash must change when a source changes.
#
# Run from verify/ after `lake build` and `scripts/check-axioms.sh` (which builds CertCheck).
set -euo pipefail
cd "$(dirname "$0")/.."
src=$(pwd)

# Hash of the Lean sources and gate scripts of a verify/ directory.
snapshot() {
	(cd "$1" && find Certify Certify.lean CertCheck.lean lakefile.toml lake-manifest.json lean-toolchain scripts -type f -print0 |
		LC_ALL=C sort -z | xargs -0 shasum -a 256) | shasum -a 256 | cut -d' ' -f1
}
initial=$(snapshot "$src")

work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
copy=$work/verify
mkdir -p "$copy/.lake"
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
grep_msg='axiom / implemented_by / extern declaration found'

# probe CHECKER I: plant I in the copy and run CHECKER on it. Prints why the gate is
# wrong and returns 1 if it accepted the plant, or rejected it without saying why.
probe() {
	local checker=$1 i=$2 out
	cp "$pristine" "$target"
	printf '\n%s\n' "${plants[$i]}" >> "$target"
	if out=$("$checker" --scan-only 2>&1); then
		echo "the scan accepted: ${plants[$i]}"
		return 1
	fi
	if ! grep -qF "${expects[$i]}" <<<"$out"; then
		echo "the scan rejected '${plants[$i]}' without reporting '${expects[$i]}': $(tail -3 <<<"$out")"
		return 1
	fi
	if out=$("$checker" 2>&1); then
		echo "the full gate accepted: ${plants[$i]}"
		return 1
	fi
	if ! grep -qF "$grep_msg" <<<"$out"; then
		echo "the full gate's source grep missed: ${plants[$i]}: $(tail -3 <<<"$out")"
		return 1
	fi
}

# --- the gate ---------------------------------------------------------------------------
for i in "${!plants[@]}"; do
	if ! why=$(probe "$copy/scripts/check-axioms.sh" "$i"); then
		echo "FAIL: $why" >&2
		exit 1
	fi
	echo "ok: ${expects[$i]} rejected by the scan and by the source grep"
done
cp "$pristine" "$target"

# --- the self-test's own assertions -----------------------------------------------------
# Each broken gate must be caught by `probe`, for the reason it is broken.
fake=$work/fake
mkdir -p "$fake"
printf '#!/usr/bin/env bash\nexit 0\n' > "$fake/accepts-all.sh"
printf '#!/usr/bin/env bash\necho "error: something"\nexit 1\n' > "$fake/unnamed.sh"
cat > "$fake/no-grep.sh" <<EOF
#!/usr/bin/env bash
echo "${expects[0]} ${expects[1]} ${expects[2]}"
exit 1
EOF
chmod +x "$fake"/*.sh
for case in "accepts-all.sh:the scan accepted" "unnamed.sh:without reporting" "no-grep.sh:source grep missed"; do
	checker=${case%%:*} reason=${case#*:}
	if why=$(probe "$fake/$checker" 0); then
		echo "FAIL: the self-test did not notice a broken gate ($checker)" >&2
		exit 1
	fi
	if ! grep -qF "$reason" <<<"$why"; then
		echo "FAIL: broken gate $checker caught for the wrong reason: $why" >&2
		exit 1
	fi
	echo "ok: a broken gate ($checker) fails the self-test: $reason"
done
cp "$pristine" "$target"
before=$(snapshot "$copy")
printf '\n-- edited\n' >> "$target"
if [ "$(snapshot "$copy")" = "$before" ]; then
	echo "FAIL: the snapshot does not see a source edit" >&2
	exit 1
fi
echo "ok: the snapshot sees a source edit"

# --- the checkout is as it was ------------------------------------------------------------
if [ "$(snapshot "$src")" != "$initial" ]; then
	echo "FAIL: the checkout's sources changed during the self-test" >&2
	exit 1
fi
echo "axiom gate self-test ok"
