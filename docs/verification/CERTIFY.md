# Certified linear static results

The Rust checker (`engine/src/certify/`) and the Lean proofs (`verify/`) implement and
prove this contract. Change both together.

## Setting

After eliminating restrained DOFs the solver solves `K u = f`, with `K` an n×n real
matrix (the assembled free-free stiffness, in f64), `f` the effective load vector.
The solver returns an approximation `ũ`. Let `u*` be the exact real solution.

`K` is *not* assumed bitwise symmetric (floating-point `Tᵀ k T` need not be).

## Theorem (coercivity error bound)

Let `S = (K + Kᵀ)/2`, `σ > 0`. If `S − σI` is positive semidefinite then for all x:

    σ‖x‖² ≤ xᵀSx = xᵀKx ≤ ‖x‖‖Kx‖     so     ‖Kx‖ ≥ σ‖x‖.

Hence K is invertible, `u*` exists and is unique, and with `r = f − Kũ`:

    ‖u* − ũ‖₂ ≤ ‖r‖₂ / σ.

## Certifying `S − σI ⪰ 0` (strictly: ≻ 0)

Run Cholesky on `M = S − σI` in interval arithmetic with outward rounding. Every
entry of the exact real `M` lies in its interval. Inclusion monotonicity means every
intermediate real quantity of exact Cholesky on `M` lies in the corresponding
interval. If every pivot interval (before the square root) has a strictly positive
lower bound, the exact real Cholesky of `M` has positive pivots, so `M` is positive
definite (Schur complement induction).

## Certifying `‖r‖₂`

Compute `r = f − Kũ` in interval arithmetic, then `ρ = ‖r‖₂` rounded up
(sum of squares of max |endpoint|, sqrt, all rounded up).

## Output

`bound = ρ / σ` rounded up, plus `rel = bound / ‖ũ‖₂` (rounded up). `σ` is chosen
heuristically (e.g. 0.9 × an estimate of λ_min(S)); soundness does not depend on how.
On failure (pivot not certified positive, non-finite value) the result is
`Uncertified { reason }`, never a wrong bound.

## Rounding model

f64 ops round to nearest, so the exact result is within 0.5 ulp of the computed one.
Outward rounding = compute in round-to-nearest, then `next_down` the lower endpoint
and `next_up` the upper. Non-finite endpoints abort the certificate. In Lean this is
modelled by abstract `down, up : ℝ → ℝ` with `down x ≤ x ≤ up x`.

## Out of scope for v1

Sparse/large models (dense interval Cholesky is O(n³); cap n), mechanism certificates
(exact rational null vectors), eigenvalue (buckling/modal) certificates, and proving
the f64 implementation matches the Lean algorithm.
