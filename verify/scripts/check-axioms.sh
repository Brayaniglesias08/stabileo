#!/usr/bin/env bash
# Trusted-base gate for the Lean proofs (docs/verification/TRUSTED_BASE.md). Run from
# verify/ after `lake build`. Fails on:
# * an unpinned toolchain or Mathlib (lean-toolchain must name an exact release, the
#   lakefile's Mathlib `rev` must be a tag or commit and match lake-manifest.json, and
#   Mathlib's own toolchain must equal ours);
# * `sorry`, `admit` or `native_decide` anywhere in the sources (native_decide trusts the
#   compiler: it adds the axiom `Lean.ofReduceBool`, and there is no justified use here);
# * an axiom beyond Lean's standard three (propext, Classical.choice, Quot.sound) behind
#   any top-level soundness theorem (`#print axioms`, printed for review), and behind any
#   constant of the Certify library at all (scripts/AxiomScan.lean, which also rejects
#   `axiom`, `@[implemented_by]` and `@[extern]` in Certify).
set -euo pipefail
cd "$(dirname "$0")/.."

# --- pins ---------------------------------------------------------------------------
toolchain=$(tr -d '[:space:]' < lean-toolchain)
if ! [[ $toolchain =~ ^leanprover/lean4:v[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
	echo "error: lean-toolchain '$toolchain' is not an exact release" >&2
	exit 1
fi
lake_rev=$(sed -n '/name = "mathlib"/,/^\[/s/^rev = "\(.*\)"/\1/p' lakefile.toml)
manifest_rev=$(jq -r '.packages[] | select(.name == "mathlib") | .inputRev' lake-manifest.json)
manifest_commit=$(jq -r '.packages[] | select(.name == "mathlib") | .rev' lake-manifest.json)
if [ -z "$lake_rev" ] || ! [[ $lake_rev =~ ^(v[0-9]+\.[0-9]+\.[0-9]+|[0-9a-f]{40})$ ]]; then
	echo "error: Mathlib rev '$lake_rev' in lakefile.toml is not a release tag or commit" >&2
	exit 1
fi
if [ "$lake_rev" != "$manifest_rev" ] || ! [[ $manifest_commit =~ ^[0-9a-f]{40}$ ]]; then
	echo "error: lake-manifest.json (inputRev $manifest_rev, rev $manifest_commit) does not pin lakefile's $lake_rev" >&2
	exit 1
fi
if [ -f .lake/packages/mathlib/lean-toolchain ]; then
	ml_toolchain=$(tr -d '[:space:]' < .lake/packages/mathlib/lean-toolchain)
	if [ "$ml_toolchain" != "$toolchain" ]; then
		echo "error: Mathlib wants $ml_toolchain, lean-toolchain says $toolchain" >&2
		exit 1
	fi
fi
echo "pins ok: $toolchain, mathlib $lake_rev ($manifest_commit)"

# --- forbidden tactics ----------------------------------------------------------------
if grep -rnwE --include='*.lean' -e 'sorry|admit|native_decide' Certify Certify.lean CertCheck.lean; then
	echo "error: sorry/admit/native_decide found" >&2
	exit 1
fi

# --- axioms of the top-level soundness theorems (printed) -------------------------------
theorems=(
	# linear static: dense interval Cholesky, Gram/Gershgorin, bundle checker
	Certify.coercive_solution
	Certify.error_bound
	Certify.coercive_of_posDef
	Certify.certified_error_bound
	Certify.posDef_of_interval_cholesky
	Certify.certified_error_bound_gram
	Certify.coercive_of_gram
	Certify.Bundle.check_sound
	Certify.Bundle.check_sound_coercive
	# interval arithmetic (abstract outward rounding down x ≤ x ≤ up x)
	Certify.mem_add
	Certify.mem_sub
	Certify.mem_mul
	Certify.mem_div
	Certify.mem_sqrt
	Certify.mem_square
	# buckling
	Certify.buckling_enclosure
	Certify.buckling_criticalFactor_mem
	# modal: counting (Sylvester/Finsler), eigenvalue inclusion, Kahan
	Certify.count_le_of_pos_on
	"Certify.count_le_of_pos_on'"
	Certify.count_le_of_pos_on_ker
	Certify.finrank_deletedSubspace
	Certify.count_le_of_deleted
	Certify.count_le_of_finsler
	Certify.count_le_of_finsler_gram
	Certify.exists_eigenvalue_near
	Certify.kahan_count_of_sep
	Certify.kahan_union_count
	Certify.kahan_lower_count
	# member forces
	Certify.abs_mulVec_sub_le
	Certify.force_bound
	# plastic collapse
	Certify.Plastic.bern_bound
	Certify.Plastic.static_sound
	Certify.Plastic.kinematic_sound
	Certify.Plastic.check_sound
	# load combinations and envelopes
	Certify.Combo.case_sound
	Certify.Combo.check_sound_disp
	Certify.Combo.check_sound_quantities
)
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
{
	echo 'import Certify'
	for t in "${theorems[@]}"; do echo "#print axioms $t"; done
} > "$tmp/Axioms.lean"
out=$(lake env lean "$tmp/Axioms.lean" 2>&1)
echo "$out"
bad=$(echo "$out" | grep -oE '\[[^]]*\]' | tr -d '[]' | tr ',' '\n' | sed 's/^ *//' |
	grep -vxE 'propext|Classical\.choice|Quot\.sound' | grep -v '^$' || true)
if [ -n "$bad" ] || echo "$out" | grep -qE 'error:|sorryAx'; then
	echo "error: unexpected axioms or errors: $bad" >&2
	exit 1
fi
if [ "$(echo "$out" | grep -c 'depends on axioms\|does not depend on any axioms')" -ne "${#theorems[@]}" ]; then
	echo "error: expected ${#theorems[@]} '#print axioms' reports" >&2
	exit 1
fi

# --- every constant of the library ------------------------------------------------------
lake env lean scripts/AxiomScan.lean
echo "axioms ok"
