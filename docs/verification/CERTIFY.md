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

## Constraints (multi-point constraints)

The constrained solver (`solve_constrained_{2d,3d}`) writes the free displacements as
`u_F = C q + u_p`, with `C` the free rows / free-independent columns of its constraint
transform, `C_R` the free rows / restrained-independent columns, `u_R` the prescribed
values and `u_p = C_R u_R`. The f64 entries of `C` (as `build_constraint_transform`
computes them, offsets included) and of the assembled `K`, `f` define the problem. The
certified statement is about the exact real solution of

    find q:  Cᵀ (f_F − K_F· [C q + u_p ; u_R]) = 0,        u*_F = C q* + u_p,

where `K_F·` is the free rows of the solver's `K` (the dense assembly on the dense path,
the sparse `k_ff`/`k_full` pair on the sparse path, chosen from the results' diagnostics
in 2D; always the sparse assembly in 3D).

* `A = Cᵀ K_FF C` is formed entrywise in interval arithmetic from the f64 `K` and `C`
  (no rounded reduced matrix is trusted); its symmetric part `Cᵀ S C` is enclosed on the
  lower triangle and certified `λ_min ≥ μ > 0` by the Gram residual + Gershgorin check
  above. Then `A` is invertible, `q*` is unique and `‖q* − q̃‖₂ ≤ ‖r‖₂ / μ`, with
  `r = Cᵀ(f_F − K_F·[C q̃ + u_p ; u_R])` evaluated in intervals.
* `q̃` is read from the returned displacements at the independent DOFs. Any `q̃` is a
  valid candidate; the mismatch is accounted for in the next step.
* Mapping back: `u*_F − ũ_F = C (q* − q̃) + (C q̃ + u_p − ũ_F)`, so

      ‖u*_F − ũ_F‖₂ ≤ ‖C‖₂ · ‖r‖₂/μ + ‖C q̃ + u_p − ũ_F‖₂,

  with `‖C‖₂ ≤ min(√(‖C‖₁‖C‖_∞), √(maxₐ Σ_b |(CᵀC)_ab|))` rounded up and the last
  term enclosed in intervals (it covers the solver's f64 expansion `C q̃ + u_p`, and any
  constraint violation in the returned displacements).

The certificate's `sigma` is `μ` and `residual_norm` is `‖r‖₂` (reduced quantities);
`abs_bound` is on the free-DOF error. Code: `engine/src/certify/constrained.rs`.

## Inclined supports

The solver rotates the translational DOFs of an inclined node by the f64 matrix `R`
(rows = local axes), `u_rot = T u_glob` with `T` block-diagonal, assembles `K`, `f` in
those axes, solves, and reports `ũ_glob = fl(Tᵀ ũ_rot)`. The certified exact global
solution is `u*_glob := Tᵀ u*_rot` (the exact real product with the f64 `R`).

The candidate `ũ_rot = fl(T ũ_glob)` (with the solver's prescribed normal value put back
on the restrained rotated DOF) is certified in the rotated system exactly as above (plain
or constrained), giving `‖u*_rot − ũ_rot‖₂ ≤ b` (restrained rotated DOFs are exact).
Then over all DOFs

    ‖u*_glob − ũ_glob‖₂ ≤ ‖T‖₂ b + ‖Tᵀ ũ_rot − ũ_glob‖₂,

with the last term enclosed in intervals and, for any real `R` (no orthogonality is
assumed), `‖R‖₂² = λ_max(RᵀR) ≤ 1 + ‖RᵀR − I‖_F` with `RᵀR − I` enclosed in intervals;
`‖T‖₂ ≤ max(1, max_R √(1 + ‖RᵀR − I‖_F))`, which is `1 + O(ε)` for the solver's
rotations. With inclined supports `abs_bound` is on the error over all DOFs in global
axes (and `rel_bound` relative to `‖ũ_glob‖₂` over all DOFs). A node carrying several
supports one of which is inclined is refused (`Uncertified`).

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

With constraints the solver solves the reduced pencil `(Cᵀ K C) φ_q = λ (Cᵀ N C) φ_q`.
The certificate is about that pencil with the f64 `K`, `N`, `C` entries taken as exact:
both reduced matrices are enclosed entrywise in interval arithmetic (as in "Constraints"
above), `A(λ) = CᵀKC − λ CᵀNC` is enclosed from those enclosures, and the lower and
upper bounds are certified exactly as above (the Rayleigh vector is the returned mode
read at the independent DOFs).

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

With constraints the same three certificates are applied to the reduced pencil
`(Cᵀ K C, Cᵀ M C)` the solver solves (f64 sparse `K`, `M`, `C` taken as exact; both
reduced matrices enclosed entrywise in intervals, every use of their entries — `Kφ`,
`Mφ`, the Gram/Gershgorin and deleted-principal-submatrix checks, the dense Finsler
matrix — goes through those enclosures), with the computed modes read at the
independent DOFs.

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

Warping frames (14 DOFs per member in a 7-DOF-per-node model) use
`frame_local_stiffness_3d_warping` the same way; rows 6 and 13 give the bimoment bound.
With constraints or inclined supports the displacement bound fed in is the mapped one
of those sections (global axes), so the force bounds hold unchanged.

**Shell stress resultants.** For MITC4 quads, DKT plates and MITC9 quads the reported
centroid resultants `σxx, σyy, τxy, mx, my, mxy` (and MITC4 `qx, qy`) are linear in the
element's local displacements, `s = L T u_e` (thermal terms are constant and cancel in
the difference). The columns `L e_k` are obtained by running the solver's own recovery
(`quad_stresses`, `quad_transverse_shear`, `plate_stress_recovery`, `quad9_stresses`) on
unit vectors with zero thermal load, and each resultant gets
`‖row_i(L)‖₂ · ‖T‖₂ · ‖u* − ũ‖₂`. As for member forces, `L` is the map *as the f64
recovery evaluates it* (its entries carry the recovery's rounding) and rounding in
evaluating `s(ũ)` is not included. Von Mises and principal stresses are not linear and
get no bound; neither do solid shells or curved shells.

Lean (`verify/Certify/Forces.lean`): `abs_mulVec_sub_le` (`|(B(u − ũ))ᵢ| ≤
‖row_i B‖ ‖u − ũ‖`) and `force_bound`: if `‖row_i k‖ ≤ r`, `‖Tx‖ ≤ τ‖x‖` for all `x`
(`τ ≥ 0`) and `‖u − ũ‖ ≤ b`, then `|((kT)(u − ũ))ᵢ| ≤ r τ b`. The bound `‖T‖₂ ≤ 1 + 4ε`
is assumed, not proved.

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
