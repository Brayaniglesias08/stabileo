#!/usr/bin/env bash
# Plant assembly bugs in the engine's source, one at a time, and check that
# the end-to-end certificate tests fail on each (docs/verification/CERTIFY.md,
# "End-to-end certificates").
#
# The bugs are planted in a temporary git worktree of the committed HEAD, never
# in this checkout; the worktree is removed on exit, including on Ctrl-C.
# Builds go to $CARGO_TARGET_DIR (default: target/e2e-planted in this checkout,
# so dependencies are reused between runs). Logs: $E2E_LOG_DIR (default: a
# temporary directory, printed).
#
#   scripts/e2e-planted-bugs.sh            # all bugs
#   scripts/e2e-planted-bugs.sh 3          # only bug #3
#
# Exit status 0 when every planted bug is caught.
set -uo pipefail
root=$(git -C "$(dirname "$0")/.." rev-parse --show-toplevel)
work=$(mktemp -d "${TMPDIR:-/tmp}/e2e-planted.XXXXXX")
logs=${E2E_LOG_DIR:-$(mktemp -d "${TMPDIR:-/tmp}/e2e-planted-logs.XXXXXX")}
export CARGO_TARGET_DIR=${CARGO_TARGET_DIR:-$root/target/e2e-planted}
cleanup() {
  git -C "$root" worktree remove --force "$work/tree" >/dev/null 2>&1
  rm -rf "$work"
}
trap cleanup EXIT
trap 'exit 130' INT TERM
git -C "$root" worktree add --detach "$work/tree" HEAD >/dev/null 2>&1 || { echo "cannot create a worktree"; exit 2; }
cd "$work/tree/engine"
echo "planting in $work/tree (HEAD $(git rev-parse --short HEAD)); logs in $logs"

# name @@ file @@ text to replace (first occurrence) @@ replacement
BUGS=(
  "2D transform: wrong sign of -sin @@ src/element/transform.rs @@     t[1 * 6 + 0] = -sin; @@     t[1 * 6 + 0] = sin;"
  "2D frame: Timoshenko (2-phi) term dropped @@ src/element/frame.rs @@ let c4 = (2.0 - phi) * ei / (l * denom); @@ let c4 = 2.0 * ei / (l * denom);"
  "3D frame: Timoshenko (4+phi) term dropped @@ src/element/frame.rs @@         let c3 = (4.0 + phi) * e * i / (l * denom); @@         let c3 = 4.0 * e * i / (l * denom);"
  "hinge: condensed stiffness factor 12/(4+phi) as 3 @@ src/element/frame.rs @@         let factor = 12.0 / (4.0 + phi); @@         let factor = 3.0;"
  "FEF: uniform-load end moment qL^2/8 @@ src/element/fef.rs @@     let mz_i_u = q_uniform * l * l / 12.0; @@     let mz_i_u = q_uniform * l * l / 8.0;"
  "FEF: Timoshenko moment correction sign @@ src/element/fef.rs @@     [mu * (v_i + phi * r_i), mu * (m_i + dm), mu * (v_j + phi * r_j), mu * (m_j - dm)] @@     [mu * (v_i + phi * r_i), mu * (m_i - dm), mu * (v_j + phi * r_j), mu * (m_j + dm)]"
  "FEF 3D: q_z moments not negated (theta_y = -dw/dx) @@ src/element/fef.rs @@     fef[4] = -fz[2];   // my_i (negative because θy = -dw/dx) @@     fef[4] = fz[2];"
  "FEF: hinge condensation ratio (2-phi) @@ src/element/fef.rs @@         let sm = (2.0 - phi) / (4.0 + phi); @@         let sm = (2.0 + phi) / (4.0 + phi);"
  "thermal: gradient moment sign @@ src/element/fef.rs @@     [-fx, 0.0, -mz, fx, 0.0, mz] @@     [-fx, 0.0, mz, fx, 0.0, -mz]"
  "3D local axes: roll angle sign @@ src/element/transform.rs @@         let angle = angle_deg * std::f64::consts::PI / 180.0; @@         let angle = -angle_deg * std::f64::consts::PI / 180.0;"
  "3D local axes: left-hand flips ez instead of ey @@ src/element/transform.rs @@         ey = [-ey[0], -ey[1], -ey[2]]; @@         ey = [ey[0], ey[1], ey[2]];"
  "3D Timoshenko: phi_y paired with Iz @@ src/solver/assembly.rs @@                 let py = sec.as_y.map(|ay| 12.0 * e * sec.iy / (g * ay * l2)).unwrap_or(0.0); @@                 let py = sec.as_y.map(|ay| 12.0 * e * sec.iz / (g * ay * l2)).unwrap_or(0.0);"
  "2D rotated spring: coupling sign @@ src/solver/assembly.rs @@                     let k_xz = (kx_val - ky_val) * s * c; @@                     let k_xz = (ky_val - kx_val) * s * c;"
  "truss load: lever rule end swapped @@ src/solver/assembly.rs @@                 ri += w * (l - xc) / l; @@                 ri += w * xc / l;"
  "thermal: alpha 12e-6 as 1.2e-6 @@ src/solver/assembly.rs @@                         let alpha = 12e-6; // Steel default @@                         let alpha = 1.2e-6;"
)

TESTS=(--test certify_end_to_end --test certify_end_to_end_mutations)
only="${1:-}"
caught=0
missed=()
for i in "${!BUGS[@]}"; do
  n=$((i + 1))
  [[ -n "$only" && "$only" != "$n" ]] && continue
  entry="${BUGS[$i]}"
  name="${entry%% @@ *}"; entry="${entry#* @@ }"
  file="${entry%% @@ *}"; entry="${entry#* @@ }"
  from="${entry%% @@ *}"; to="${entry#* @@ }"
  if ! python3 - "$file" "$from" "$to" <<'PY'
import sys
p, a, b = sys.argv[1:]
s = open(p).read()
if a not in s:
    sys.exit(1)
open(p, "w").write(s.replace(a, b, 1))
PY
  then
    echo "#$n $name: pattern not found in $file"
    missed+=("#$n $name (pattern not found)")
    continue
  fi
  if cargo test --release -q "${TESTS[@]}" >$logs/planted-$n.log 2>&1; then
    echo "#$n $name: NOT CAUGHT"
    missed+=("#$n $name")
  else
    if grep -q "could not compile" $logs/planted-$n.log; then
      echo "#$n $name: does not compile"
      missed+=("#$n $name (compile error)")
    else
      fails=$(awk '/^failures:$/{f=1;next} f&&/^    [a-z_0-9]+$/{print $1} /^test result/{f=0}' $logs/planted-$n.log | sort -u | tr '\n' ' ')
      echo "#$n $name: caught by $fails"
      caught=$((caught + 1))
    fi
  fi
  git checkout -q -- "$file"
done
echo "caught $caught; missed ${#missed[@]}"
for m in "${missed[@]}"; do echo "  missed: $m"; done
[[ ${#missed[@]} -eq 0 ]]
