#!/usr/bin/env bash
# Plant assembly bugs in the engine's source, one at a time, and check that
# the end-to-end certificate tests fail on each (docs/verification/CERTIFY.md,
# "End-to-end certificates"). The source is restored after every bug.
#
#   scripts/e2e-planted-bugs.sh            # all bugs
#   scripts/e2e-planted-bugs.sh 3          # only bug #3
#
# Exit status 0 when every planted bug is caught.
set -uo pipefail
cd "$(dirname "$0")/../engine"

# name | file | text to replace (first occurrence) | replacement
BUGS=(
  "2D transform: wrong sign of -sin|src/element/transform.rs|    t[1 * 6 + 0] = -sin;|    t[1 * 6 + 0] = sin;"
  "2D frame: Timoshenko (2-phi) term dropped|src/element/frame.rs|let c4 = (2.0 - phi) * ei / (l * denom);|let c4 = 2.0 * ei / (l * denom);"
  "3D frame: Timoshenko (4+phi) term dropped|src/element/frame.rs|        let c3 = (4.0 + phi) * e * i / (l * denom);|        let c3 = 4.0 * e * i / (l * denom);"
  "hinge: condensed stiffness factor 12/(4+phi) as 3|src/element/frame.rs|        let factor = 12.0 / (4.0 + phi);|        let factor = 3.0;"
  "FEF: uniform-load end moment qL^2/8|src/element/fef.rs|    let mz_i_u = q_uniform * l * l / 12.0;|    let mz_i_u = q_uniform * l * l / 8.0;"
  "FEF: Timoshenko moment correction sign|src/element/fef.rs|    [mu * (v_i + phi * r_i), mu * (m_i + dm), mu * (v_j + phi * r_j), mu * (m_j - dm)]|    [mu * (v_i + phi * r_i), mu * (m_i - dm), mu * (v_j + phi * r_j), mu * (m_j + dm)]"
  "FEF 3D: q_z moments not negated (theta_y = -dw/dx)|src/element/fef.rs|    fef[4] = -fz[2];   // my_i (negative because θy = -dw/dx)|    fef[4] = fz[2];"
  "FEF: hinge condensation ratio (2-phi)|src/element/fef.rs|        let sm = (2.0 - phi) / (4.0 + phi);|        let sm = (2.0 + phi) / (4.0 + phi);"
  "thermal: gradient moment sign|src/element/fef.rs|    [-fx, 0.0, -mz, fx, 0.0, mz]|    [-fx, 0.0, mz, fx, 0.0, -mz]"
  "3D local axes: roll angle sign|src/element/transform.rs|        let angle = angle_deg * std::f64::consts::PI / 180.0;|        let angle = -angle_deg * std::f64::consts::PI / 180.0;"
  "3D local axes: left-hand flips ez instead of ey|src/element/transform.rs|        ey = [-ey[0], -ey[1], -ey[2]];|        ey = [ey[0], ey[1], ey[2]];"
  "3D Timoshenko: phi_y paired with Iz|src/solver/assembly.rs|                let py = sec.as_y.map(|ay| 12.0 * e * sec.iy / (g * ay * l2)).unwrap_or(0.0);|                let py = sec.as_y.map(|ay| 12.0 * e * sec.iz / (g * ay * l2)).unwrap_or(0.0);"
  "2D rotated spring: coupling sign|src/solver/assembly.rs|                    let k_xz = (kx_val - ky_val) * s * c;|                    let k_xz = (ky_val - kx_val) * s * c;"
  "truss load: lever rule end swapped|src/solver/assembly.rs|                ri += w * (l - xc) / l;|                ri += w * xc / l;"
  "thermal: alpha 12e-6 as 1.2e-6|src/solver/assembly.rs|                        let alpha = 12e-6; // Steel default|                        let alpha = 1.2e-6;"
)

TESTS=(--test end_to_end_certificates --test end_to_end_mutations)
only="${1:-}"
caught=0
missed=()
for i in "${!BUGS[@]}"; do
  n=$((i + 1))
  [[ -n "$only" && "$only" != "$n" ]] && continue
  IFS='|' read -r name file from to <<<"${BUGS[$i]}"
  cp "$file" "$file.orig"
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
    mv "$file.orig" "$file"
    missed+=("#$n $name (pattern not found)")
    continue
  fi
  if cargo test --release -q "${TESTS[@]}" >/tmp/e2e-planted-$n.log 2>&1; then
    echo "#$n $name: NOT CAUGHT"
    missed+=("#$n $name")
  else
    if grep -q "error\[E" /tmp/e2e-planted-$n.log; then
      echo "#$n $name: does not compile"
      missed+=("#$n $name (compile error)")
    else
      fails=$(grep -E "^test .* FAILED$" /tmp/e2e-planted-$n.log | sed 's/^test //; s/ \.\.\. FAILED//' | tr '\n' ' ')
      echo "#$n $name: caught by $fails"
      caught=$((caught + 1))
    fi
  fi
  mv "$file.orig" "$file"
done
echo "caught $caught; missed ${#missed[@]}"
for m in "${missed[@]}"; do echo "  missed: $m"; done
[[ ${#missed[@]} -eq 0 ]]
