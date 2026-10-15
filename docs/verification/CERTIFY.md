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

## Scalable certificate (Gram residual + Gershgorin)

For sparse/large systems the dense interval Cholesky is replaced by a check that is
linear in the number of nonzeros of `L` and `R`. The checker picks *any* real matrix
`L` — in practice the floating-point sparse Cholesky factor of `S − tI` for some shift
`t`, with rows permuted — and encloses the residual entrywise in intervals:

    R = S − L Lᵀ,   Rᵢⱼ ∈ IRᵢⱼ   for all i, j.

`R` is symmetric (both `S` and `L Lᵀ` are), and `xᵀ L Lᵀ x = ‖Lᵀx‖² ≥ 0` for every
real `L`. If for every row i

    μ ≤ IRᵢᵢ.lo − ∑_{j≠i} mag(IRᵢⱼ)        (interval Gershgorin row test)

then `μ‖x‖² ≤ xᵀRx` (from `|xᵢxⱼ| ≤ (xᵢ² + xⱼ²)/2` and symmetry of `R`), so

    xᵀKx = xᵀSx = xᵀ L Lᵀ x + xᵀRx ≥ μ‖x‖²,   i.e.  λ_min(S) ≥ μ.

If `μ > 0`, `σ := μ` certifies coercivity and the error bound above applies:
`‖u* − ũ‖₂ ≤ ρ / μ`. Soundness is independent of how `L` was computed: its fill-reducing
ordering, rounding errors, underflow, FMA, or even whether it is triangular only affect
the size of `R` (hence whether the test passes), never the validity of the bound. Only
the enclosure of `S − L Lᵀ` must be computed with outward rounding.

Lean: `Certify.certified_error_bound_gram` (`verify/Certify/Gram.lean`), built on
`quad_gram_nonneg`, `gershgorin_quad` and `coercive_of_gram`.

## Buckling load-factor enclosure

Linear buckling solves `K φ = λ N φ` with `N = −K_G`. The stability matrix
`A(λ) = K − λN` is affine in λ, and the structure is *stable* at λ when `A(λ) ≻ 0`
(`xᵀA(λ)x > 0` for all `x ≠ 0`). The critical load factor is where positive
definiteness is first lost:

    λ_cr = sup { λ ≥ 0 : A(λ') ≻ 0 for all λ' ∈ [0, λ] }.

* **Lower bound (convexity).** `xᵀA(λ)x = xᵀKx − λ xᵀNx` is affine in λ, so if
  `A(0) ≻ 0` and `A(λ̲) ≻ 0` then `A(λ) ≻ 0` on all of `[0, λ̲]`, hence `λ_cr ≥ λ̲`.
  Both endpoints are certified with the Gram residual + Gershgorin check above
  (applied to the interval-enclosed entries of `K − λN`, `μ > 0`).
* **Upper bound (Rayleigh quotient).** For the computed mode `φ ≠ 0`, if
  `φᵀKφ ∈ Inum`, `φᵀNφ ∈ Iden`, `Iden.lo > 0` and `λ̄ ≥ max(0, Inum.hi / Iden.lo)`
  (rounded up), then `φᵀA(λ̄)φ ≤ 0`, so `A(λ̄)` is not positive definite and `λ_cr ≤ λ̄`.

The enclosure `λ̲ ≤ λ_cr ≤ λ̄` is about the f64 `K` and `K_G` as assembled (with `K_G`
built from the computed linear axial forces), not about rounding-free versions of them.
Constraints are not supported.

Lean: `Certify.buckling_enclosure` (`verify/Certify/Buckling.lean`), built on
`stable_segment`, `not_stable_of_interval_rayleigh` and `stable_of_gram`; the
corollary `Certify.buckling_criticalFactor_mem` states `λ_cr ∈ [λ̲, λ̄]` for
`criticalFactor K N := sSup {λ | 0 ≤ λ ∧ ∀ λ' ∈ [0, λ], Stable K N λ'}`.

## Modal enclosures and completeness

Free vibration solves `K φ = λ M φ` (`λ = ω²`) with `K ≻ 0` and `M ⪰ 0` (possibly
singular). `engine/src/certify/modal.rs` certifies three things.

* **Single-mode enclosure.** With `C = K^{-1/2} M K^{-1/2}` (symmetric, eigenvalues
  `ν = 1/λ`) and `y = K^{1/2}φ`, the residual `C y − ν̂ y = K^{-1/2} s`,
  `s = Mφ − ν̂ Kφ`. For a symmetric matrix some eigenvalue `ν` satisfies
  `|ν − ν̂| ≤ ‖C y − ν̂ y‖₂ / ‖y‖₂` (Bauer–Fike for normal matrices), and
  `‖K^{-1/2}s‖/‖y‖ ≤ ‖s‖ / √(σ_K · φᵀKφ)` with `σ_K` a certified lower bound on `λ_min(K)`.
* **At most `j` eigenvalues `λ ≤ μ`.** Stated without eigenvalues (min-max form): every
  subspace `S` on which `xᵀ(K − μM)x ≤ 0` has `dim S ≤ j`. If `A = K − μM` is positive
  on a subspace `V` then `S ∩ V = 0`, so `dim S + dim V ≤ n`. Two certified choices
  of `V`: the coordinate subspace `{x | xᵢ = 0, i ∈ I}` (a principal submatrix of `A`
  with `|I| = j` rows/columns deleted is positive definite, `dim V = n − |I|`), or
  `V = ker Uᵀ` when `A + c U Uᵀ ≻ 0` for some `U ∈ ℝ^{n×j}` (Finsler; checked by the
  Gram residual + Gershgorin certificate).
* **At least `j` eigenvalues `λ < μ`.** Kahan's block residual theorem applied to
  `Y = K^{1/2}Φ`, `R = CY − Y diag(ν̂)`: `j` eigenvalues of `C` (with multiplicity) lie
  within `‖R‖₂ / σ_min(Y)` of the `ν̂ᵢ`. This is a **cited theorem, not yet formalized**.

Lean (`verify/Certify/Modal.lean`), with `NonposSubspace A S := ∀ x ∈ S, xᵀAx ≤ 0`:

* `count_le_of_pos_on` / `count_le_of_pos_on'`: `A` positive on `V \ 0` ⇒ every
  nonpositive `S` has `dim S + dim V ≤ n` (`dim S ≤ n − dim V`).
* `finrank_deletedSubspace`: `dim {x | xᵢ = 0, i ∈ I} = n − |I|`;
  `count_le_of_deleted`: `A` positive there ⇒ `dim S ≤ |I|`. (Positivity of the
  principal submatrix is taken as a hypothesis; its Gram/Gershgorin certificate is not
  connected in Lean.)
* `count_le_of_finsler`: `A + c U Uᵀ ≻ 0` ⇒ `dim S ≤ j`; `count_le_of_finsler_gram`:
  the same from an interval Gram-residual certificate of `K − μM + c U Uᵀ` (any real `L`,
  Gershgorin bound `> 0`).
* `exists_eigenvalue_near`: for real symmetric `C`, `y ≠ 0` and any `ν`, some
  `e = hC.eigenvalues k` has `|e − ν| ≤ ‖Cy − νy‖₂ / ‖y‖₂` (via the spectral theorem).
  The change of variables to the pencil (`K^{±1/2}`, `σ_K`) is not formalized.

## Member force bounds

Local member end forces are affine in the element end displacements,
`f = k_local · T · u_e − fef`, so forces at the exact `u*` and the computed `ũ` differ
by `k_local T (u* − ũ)`. By Cauchy–Schwarz, component `i` satisfies

    |(k T (u* − ũ))ᵢ| ≤ ‖row_i(k)‖₂ · ‖T‖₂ · ‖u* − ũ‖₂,

with `‖T‖₂ ≤ 1 + 4ε` (rotation with rounded entries) and `‖u* − ũ‖₂` from the
displacement certificate (`engine/src/certify/forces.rs`). Rounding in the f64 force
evaluation itself is not included.

Lean (`verify/Certify/Forces.lean`): `abs_mulVec_sub_le` (`|(B(u − ũ))ᵢ| ≤
‖row_i B‖ ‖u − ũ‖`) and `force_bound`: if `‖row_i k‖ ≤ r`, `‖Tx‖ ≤ τ‖x‖` for all `x`
(`τ ≥ 0`) and `‖u − ũ‖ ≤ b`, then `|((kT)(u − ũ))ᵢ| ≤ r τ b`. The bound `‖T‖₂ ≤ 1 + 4ε`
is assumed, not proved.

## Load combinations and envelopes

`engine/src/certify/combinations.rs` (wasm: `certify_multi_case_2d/3d`). A combination
is `Σ cᵢ · (case i)`; its exact value is `Σ cᵢ u*ᵢ` — the exact solution for the combined
loads, since the reduced system is linear in `f` (prescribed displacements enter each
case's right-hand side and combine the same way).

* **Cases.** All cases share one stiffness, so its coercivity (σ from the dense interval
  Cholesky, or μ from the Gram + Gershgorin check) is certified once
  (`certify_load_cases_2d/3d`); each case then costs one interval residual, giving
  `‖u*ᵢ − ũᵢ‖₂ ≤ bᵢ`. `certify_results_2d/3d` are the one-case special case.
* **Combination displacements.** `‖Σcᵢu*ᵢ − ũ_c‖₂ ≤ Σ|cᵢ|bᵢ + ‖Σcᵢũᵢ − ũ_c‖₂`. The
  second term is the f64 rounding of the combination itself: `Σcᵢũᵢ` is enclosed in
  interval arithmetic and compared with the *reported* `ũ_c`, so a misreported
  combination is caught (the bound grows to cover it). All sums are rounded up.
  Factors that are exactly 0 are skipped; any uncertified case with a non-zero factor
  makes the combination uncertified.
* **Combination member forces.** End forces are affine in `u` with load terms that do
  not depend on `u` (`f = k T u − fef`), so `|F_c* − F̃_c| ≤ Σ|cᵢ|ΔFᵢ + |Σcᵢ F̃ᵢ − F̃_c|`
  per component, with `ΔFᵢ` from the member force bounds below.
* **Diagram stations.** The diagram evaluators compute `M(x) = M_start − V_start·x +
  (load terms)`, `V(x) = V_start + …`, `N(x) = N_start + …`, `T(x)` linear between ends,
  so at every station the error is at most `ΔM + ΔV·L`, `ΔV`, `ΔN`, `ΔT` (3D: `M_z` uses
  `ΔV_y`, `M_y` uses `ΔV_z`). These are the `stationBounds`.
* **Envelopes.** `max_k`, `min_k` and max-abs over combinations are 1-Lipschitz in the
  sup norm, so every envelope value (max-abs results, envelope diagram ordinates) is
  within the largest combination bound of the exact envelope. The envelope is
  certified only if every combination is, and only for members bounded in every
  combination. Max-abs displacements are bounded in magnitude (the sign may flip on a
  tie).

Not included (inherited from the member force bounds): f64 rounding in each case's
force formulas and in evaluating the diagram polynomials. Reactions are not bounded.
Constraints and inclined supports are unsupported, as for single cases.

## Certified design checks

`engine/src/certify/design.rs`, `engine/src/certify/real.rs`. A utilization ratio is a
function of the design forces; given certified enclosures `[F − ΔF, F + ΔF]` (from the
member force / combination station bounds above) the code formulas are evaluated in
outward-rounded interval arithmetic, so `[lower, upper]` contains the exact real
utilization for *every* force in the box. `certifiedOk = upper < 1` certifies the member
passes; `certifiedFail = lower > 1` certifies it fails; otherwise the result is
inconclusive at this precision.

* **One implementation.** The formulas are written once, generic over a small trait
  `Real` implemented for `f64` (the ordinary check) and `Interval`. Data enter as
  `T::exact`, decimal code constants (0.877, 4.71, φ factors …) as `T::lit` (the
  interval version widens by one ulp each way, enclosing the real decimal constant), π
  as a one-ulp enclosure. A test checks the two instantiations agree on point inputs
  (enclosure contains the f64 value, width ≤ 1e-12 relative) over thousands of random
  members, so they cannot drift.
* **Branches.** Comparisons return `True / False / Unknown`. `branch(c, a, b)` evaluates
  one arm when decided and the hull of both when the box straddles the condition
  (e.g. AISC H1-1a/H1-1b at `Pr/Pc = 0.2`, which is discontinuous; E3 inelastic/elastic;
  F2 plastic/inelastic/elastic LTB). `min`, `max`, `abs` are exact on intervals.
* **Transcendentals.** `x^y` (AISC E3-2 `0.658^(Fy/Fe)`) is evaluated at the corners of
  the box (monotone in each argument for `x > 0`) and widened by 8 ulps relative plus
  the smallest normal: this **assumes the platform libm `pow` is accurate to 8 ulps**
  (mainstream libms are within 1–2). `sqrt` is correctly rounded by IEEE 754.
* **Refusal.** A demand whose value or bound is non-finite or negative, a capacity not
  certified positive where a demand exists, or any non-finite enclosure makes the
  member `certified: false` with a reason — never a ratio.
* **Members over all demand points.** A member is certified over every demand set given
  for it (max over sets of the per-set enclosures). `steel_demands_from_combinations_*`
  supplies N, M, V (3D: N, M_y, M_z, V_y) at evenly spaced stations (default 21, as the
  envelope diagrams) of every combination with that combination's station bounds;
  an uncertified combination is an error and a member missing a bound in some
  combination is refused. Values *between* stations are not covered (as in the f64
  envelope).

Wasm: `certify_steel_members` (bounded demands in), `certify_steel_multi_case_2d/3d`
(multi-case model + members in, `{ analysis, steel }` out).

| Check | Status |
|---|---|
| AISC 360 (`steel_check`): tension D2 (yield + rupture), compression E3, flexure F2 (LTB) / F6, shear G2, interaction H1 | **certified** |
| EC3 (`ec3_check`), CIRSOC 201 (`cirsoc201_check`), RC (`rc_check`, `ec2_check`), timber, masonry, CFS, connections, foundations, serviceability | not certified (f64 only) |

Inherited limitations: rounding in the f64 evaluation of member forces and diagram
polynomials (see member force bounds) is not included; capacities are certified as the
real-number value of the code formula for the given section data.

## Rounding model

f64 ops round to nearest, so the exact result is within 0.5 ulp of the computed one.
Outward rounding = compute in round-to-nearest, then `next_down` the lower endpoint
and `next_up` the upper. Non-finite endpoints abort the certificate. In Lean this is
modelled by abstract `down, up : ℝ → ℝ` with `down x ≤ x ≤ up x`.

## Out of scope for v1

Mechanism certificates (exact rational null vectors), a Lean proof of Kahan's
theorem (used for the modal lower count), and proving the f64 implementation matches the Lean algorithm. (Large
models are covered by the Gram/Gershgorin certificate above; the dense interval
Cholesky is used only below the solver's sparse threshold.)
