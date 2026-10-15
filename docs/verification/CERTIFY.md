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
Not yet formalized in Lean (the coercivity and Gram/Gershgorin steps are the ones
proved above; the `‖C‖₂` mapping is elementary but unproved there).

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
supports one of which is inclined is refused (`Uncertified`). Not yet formalized in Lean.
## Independent checking: certificate bundles

The scalable certificate can be exported as a *bundle* and re-checked by a third party
without trusting the Rust interval code, the rounding model, or the f64 factorization.
The re-check is an executable Lean function whose soundness is a Lean theorem.

**Export (Rust).** `certify::bundle::export_linear_bundle(k, f, u)` runs the Gram
residual + Gershgorin certificate and writes the data it used;
`export_linear_bundle_{2d,3d}(input)` solve a model and export the bundle of its reduced
free-DOF system from the sparse assembly (any model size). Wasm:
`export_linear_bundle_{2d,3d}(json) -> bundle JSON`.

**Format** (`stabileo.linear-certificate.v1`, JSON). Every f64 is written exactly as an
integer pair `[m, e]` meaning `m · 2^e` (`m` odd, or `[0, 0]`), so the checker sees the
exact dyadic rationals the engine used.

| field | meaning |
|---|---|
| `n` | number of unknowns |
| `k` | `{colPtr, rowIdx, values}`: lower triangle of the symmetric `K` (CSC); each stored `(a, b, v)` means `K[a][b] = K[b][a] = v` |
| `fLo`, `fHi` | enclosure `fLo ≤ f ≤ fHi` of the right-hand side (`f_F − K_FR u_R`; a point unless displacements are prescribed) |
| `u` | the computed solution `ũ` |
| `perm` | fill-reducing permutation, `perm[new] = old` |
| `l` | `{colPtr, rowIdx, values}`: the f64 Cholesky factor `L` of `P K Pᵀ − tI` in permuted numbering; it denotes `L'` with `L'[perm[a]][c] = L[a][c]` |
| `mu` | the claimed `μ`, `λ_min(K) ≥ μ > 0` |
| `bound` | the claimed bound `b`: `‖K⁻¹f − ũ‖₂ ≤ b` |
| `info` | display copies of μ, b, ρ, relative bound (ignored by the checker) |

**Check (Lean, `lake exe certcheck <bundle.json>...`).** `Certify.Bundle.check`
(`verify/Certify/BundleCheck.lean`, core Lean + `Std` only, so it compiles natively
without Mathlib) works in exact arithmetic on dyadic rationals (`Dy`: `m · 2^e` with a
GMP `Int` mantissa; no gcd per operation, unlike `Rat`):

1. `R = K − L' L'ᵀ` exactly, one dense column of its lower triangle at a time, in fixed
   point (`L` scaled to integers by `2^-E`, `E` the smallest exponent in `L`; `R` by
   `2^-E2`, `E2 = min(2E, smallest exponent in K)`), from the columns and the
   (transposed) rows of `L'`; the cost is that of forming `L Lᵀ`, `Σ_c nnz(L_c)²/2`
   exact multiply-adds;
2. Gershgorin, for every row: `μ ≤ Rᵢᵢ − Σ_{j≠i} |Rᵢⱼ|`, accumulated into one array;
3. `K ũ` exactly, the residual interval `[fLo − Kũ, fHi − Kũ]` and
   `ρ² = Σᵢ max(|loᵢ|, |hiᵢ|)²`;
4. `0 < μ`, `0 ≤ b` and `ρ² ≤ (b μ)²` — no square roots (`√ρ² / μ ≤ b` follows:
   `Certify.Bundle.sqrt_div_le_of_sq_le`).

Exit status 0 if every bundle is accepted, 1 if one is rejected, 2 on a parse error.
The JSON reader (`verify/Certify/BundleParse.lean`) is trusted: it only turns `[m, e]`
into the dyadic `m · 2^e` and CSC into column lists.

Timings (Apple Silicon, one core, `check` only; the machine was shared):

| model | n | nnz(K) | nnz(L) | `certcheck` |
|---|---|---|---|---|
| `ex-3d-portal-frame` | 24 | 88 | 159 | < 1 ms |
| `ex-3d-tower` | 48 | 236 | 557 | 2 ms |
| `ex-3d-nave-industrial` | 1308 | 6 041 | 49 359 | 0.5 s |
| 6×6×8 frame building (generated) | 2352 | 13 048 | 365 762 | 11 s |
| 8×8×12 frame building (generated) | 5832 | 33 048 | 1 525 164 | 80 s (0.8 GB) |

The cost is dominated by the bignum multiply-adds of step 1 (about 90 ns each); the
f64 export of the same bundles takes 4 ms to 4 s.

**Soundness (`verify/Certify/BundleSound.lean`).**

* `Certify.Bundle.check_sound`: if `check d = true`, then for the real matrix
  `K = kMat d` and vector `ũ = uVec d` denoted by the bundle and *every* `f` with
  `fLo ≤ f ≤ fHi`: `K (K⁻¹ f) = f`, every solution of `K u = f` equals `K⁻¹ f`, and
  `‖K⁻¹ f − ũ‖₂ ≤ bound`. The proof applies `certified_error_bound_gram` with
  `L = lMat d`, point enclosures of the exact `R` (`resid_eq`), the residual interval
  above and `ρ = b μ`. The computation is related to the mathematics by `Dy.val_add`,
  `Dy.val_mul`, … (dyadic arithmetic is exact), `intAt_val` (the fixed-point encoding is
  exact), `resCol_val` (each dense column is the exact residual column), `gershSums_val`
  (the Gershgorin array holds exactly `Rₜₜ − Σ_{j≠t} |Rₜⱼ|`) and `mulVec_eq` (`K ũ`).
* `Certify.Bundle.check_sound_coercive`: `check d = true → μ‖x‖² ≤ xᵀKx` for all `x`.

Nothing about `L` or `perm` is checked: `L' L'ᵀ ⪰ 0` for every real `L'`, so a wrong
factor or a non-permutation can only make the Gershgorin test fail.

**Tests.** `engine/tests/certify_bundle.rs` (`export_bundles`) writes bundles for three
fixtures (`ex-3d-portal-frame`, `ex-3d-tower`, `ex-3d-nave-industrial`) to
`target/cert-bundles/`, plus three tampered copies of each whose claims are false or
unsupportable: `bound / 1024`, `ũ` moved by `4b` in one component with `b` kept (then
`‖u* − ũ'‖ ≥ 3b`, so acceptance would contradict `check_sound`), and `μ = 2 min Kᵢᵢ`
(`> λ_min`, contradicting `check_sound_coercive`). `verify/scripts/check-bundles.sh`
regenerates them, runs `certcheck`, and requires every original to be accepted and
every tampered copy rejected. The ignored test `export_large_bundles` writes the generated
frame buildings above to `target/cert-bundles-large/`.

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
* **At least `j` eigenvalues `λ < μ`.** Kahan's block residual theorem (Frobenius
  form) applied to `Y = K^{1/2}Φ`, `R = CY − Y diag(ν̂)`: if `‖Yz‖² ≥ g‖z‖²` (`g > 0`)
  and `‖R‖_F ≤ r`, at least `j` eigenvalues of `C` (with multiplicity) lie in
  `⋃ᵢ [ν̂ᵢ − ρ, ν̂ᵢ + ρ]`, `ρ = r/√g`; in particular at least `j` are `≥ min ν̂ − ρ`.
  The Rust takes `r² = Σᵢ ‖sᵢ‖²/σ_K` (column `i` of `R` is `K^{-1/2}sᵢ`, so this bounds
  `‖R‖_F²`, not just `‖R‖₂²`) and `g` a Gershgorin lower bound on `λ_min(ΦᵀKΦ) =
  σ_min(Y)²`, i.e. exactly the proved constant. Then `ν ≥ min ν̂ − ρ > 0` gives
  `λ = 1/ν ≤ 1/(min ν̂ − ρ) < μ`. Proof: in eigen-coordinates `R̂ₖᵢ = (λₖ − ν̂ᵢ)Ŷₖᵢ`; if
  fewer than `j` eigenvalues qualify, some `z ≠ 0` has `Yz` orthogonal to their
  eigenvectors, and Cauchy–Schwarz gives `g‖z‖² ≤ ‖Yz‖² < (r²/ρ²)‖z‖² = g‖z‖²`.

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
* `kahan_count_of_sep` (`verify/Certify/Kahan.lean`): `C` symmetric, `Y : n × m`,
  `∀ z, g‖z‖² ≤ ‖Yz‖²` (`g > 0`), `Σᵢⱼ (CY − Y diag ν)ᵢⱼ² ≤ r²`; if every eigenvalue
  `λₖ` with `¬P k` has `r² < g(λₖ − νⱼ)²` for all `j`, then `m ≤ #{k | P k}`.
  Corollaries (`r ≥ 0`, `ρ = r/√g`): `kahan_union_count`, `m ≤ #{k | ∃ j, |λₖ − νⱼ| ≤ ρ}`;
  `kahan_lower_count`, with `νmin ≤ νⱼ` for all `j`, `m ≤ #{k | νmin − ρ ≤ λₖ}` (the
  form the Rust uses). As above, the pencil change of variables (`C = K^{-1/2}MK^{-1/2}`,
  `‖K^{-1/2}s‖² ≤ ‖s‖²/σ_K`, `ΦᵀKΦ = YᵀY`, Gershgorin for `g`) is not formalized.

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
get no bound; neither do solid shells or curved shells. Warping and shell bounds are
not yet formalized in Lean (they are instances of `force_bound` with a different `k`).

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
* **Transcendentals.** `x^y` (AISC E3-2 `0.658^(Fy/Fe)`, EC2 `(100ρ fck)^(1/3)`, `k^1.5`)
  is evaluated at the corners of the box (monotone in each argument for `x > 0`) and
  widened by 8 ulps relative plus the smallest normal; `tan` (EC2 strut angle) likewise
  on `(−1.57, 1.57)`, refusing outside. This **assumes the platform libm `pow`/`tan` are
  accurate to 8 ulps** (mainstream libms are within 1–2). `sqrt` is correctly rounded by
  IEEE 754. An exponent written `1.0/3.0` is enclosed as the interval quotient, which
  contains the real 1/3.
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

Wasm: `certify_steel_members`, `certify_ec3_members`, `certify_ec2_members` (bounded
demands in; `ec3_demands` / `ec2_demands` convert the station demand sets and
`scale_demands` changes units, e.g. kN → N for EC2's Pa-based formulas, widening each
bound by the scaling's rounding), `certify_steel_multi_case_2d/3d`
(multi-case model + members in, `{ analysis, steel }` out).

| Check | Status |
|---|---|
| AISC 360 (`steel_check`): tension D2 (yield + rupture), compression E3, flexure F2 (LTB) / F6, shear G2, interaction H1 | **certified** |
| EC3 (`ec3_check`): tension 6.2.3, flexural buckling 6.3.1 (χ curves a–d), LTB 6.3.2, weak-axis flexure 6.2.5, shear 6.2.6, simplified interaction 6.3.3 | **certified** |
| EC2 (`ec2_check`): RC flexure 6.1 (rectangular block, singly / doubly reinforced, compression-steel yield branch), shear 6.2 (VRd,c with the NEd axial term over the whole NEd interval, VRd,s, VRd,max) | **certified** |
| CIRSOC 201 (`cirsoc201_check`), ACI RC (`rc_check`), timber, masonry, CFS, connections, foundations, serviceability | not certified (f64 only) |

EC3 note: an unbraced length `lb = 0` gives `Mcr = x/0`; the f64 check reads it as no
LTB reduction, the interval check refuses (division by an interval containing 0).

Inherited limitations: rounding in the f64 evaluation of member forces and diagram
polynomials (see member force bounds) is not included; capacities are certified as the
real-number value of the code formula for the given section data.

## P-Delta (second-order) displacements

`engine/src/certify/pdelta.rs` (wasm: `certify_pdelta_2d/3d`).

**Model.** The P-Delta solver iterates `(K + K_G(u)) u = f`, the geometric stiffness
built from axial forces read back from the displacements. Element by element
(`KgAffineTerm`, `geometric_stiffness_terms_{2d,3d}`):

    K_G(u) = Σ_e N_e(u) G_e,    N_e(u) = e·a·(t_eᵀu / l − α·ΔT),

with `G_e` the element's global geometric stiffness at unit axial force computed by the
same element routine the solver uses (frames: Przemieniecki, 3D condensed for end
releases; trusses/cables: string stiffness), `t_e` the elongation coefficients (entries of
the element's transformation), and `e`, `a`, `l`, `α`, `ΔT` the f64 element data. The
certified model takes these f64 values, the sparse triplet-assembled `K` (full, free DOFs
first) and the assembled `f` as exact; the solver's own f64 `K_G(ũ)` equals `Σ N_e G_e`
only up to rounding, which does not matter because `ũ` is only a candidate. (The small
dense path, `n_total < 64`, iterates on the dense assembly, which differs from the
triplet one by rounding.) Restrained DOFs keep their returned (prescribed) values `u_R`.
Constraints and inclined supports are refused.

The free equations `F(x) = [(K + K_G(u))u − f]_F`, `u = [x; u_R]`, are *exactly quadratic*:
with `c_e = e·a/l` and `x = ũ + δ`,

    F(ũ + δ) = F(ũ) + J δ + q(δ, δ),
    J = (K + K_G(ũ))_FF + Σ_e c_e (G_e ũ)_F t_eᵀ,     q(x, y) = Σ_e c_e (t_eᵀx)(G_e y)_F,

because `K_G` is affine in `u` (`K_G(ũ + δ) = K_G(ũ) + Σ c_e (t_eᵀδ) G_e`). `J` is not
symmetric.

**Bounds.**

* `‖q(x, y)‖₂ ≤ β‖x‖₂‖y‖₂` with `β = max_d Σ_{e ∋ d} w_e`, `w_e = |c_e|·‖t_e‖₂·‖G_e‖_F`
  (restricted to free DOFs). Proof: `‖q(x,y)‖ ≤ Σ_e w_e ‖x_e‖‖y_e‖ ≤ (Σ w_e‖x_e‖²)^{½}
  (Σ w_e‖y_e‖²)^{½}` (Cauchy–Schwarz) and `Σ_e w_e‖x_e‖² = Σ_d x_d² Σ_{e∋d} w_e ≤ β‖x‖²`.
* `xᵀJx ≥ μ‖x‖²`: `sym(J)` (same sparsity as `K`: each `c_e g_e t_eᵀ` lives on the
  element's DOFs) is enclosed entrywise in intervals and certified by the Gram residual +
  Gershgorin check. Then `‖Jx‖ ≥ μ‖x‖`, `J` is invertible and `‖J⁻¹‖₂ ≤ 1/μ`.
* `‖J⁻¹F(ũ)‖₂ ≤ η := ‖d‖₂ + ‖F(ũ) − J d‖₂ / μ` for *any* vector `d` (an f64 Newton
  step: Cholesky of `sym(J)` plus refinement with `J`; never trusted), since
  `J⁻¹F = d + J⁻¹(F − Jd)`. `F(ũ)` and `J d` are evaluated in interval arithmetic.

**Lemma (Newton–Kantorovich for a quadratic map).** Let `F(ũ + δ) = F(ũ) + Jδ + q(δ,δ)`
with `q` bilinear, `J` invertible, `‖J⁻¹F(ũ)‖ ≤ η`, `‖J⁻¹q(x,y)‖ ≤ γ‖x‖‖y‖`. If
`4γη < 1`, then `F` has exactly one zero `u*` in the open ball `‖u − ũ‖ < 1/(2γ)`, and
`‖u* − ũ‖ ≤ r := 2η / (1 + √(1 − 4γη))` (the smaller root of `γr² − r + η = 0`).

*Proof.* Zeros of `F(ũ + ·)` are the fixed points of `T(δ) = −J⁻¹F(ũ) − J⁻¹q(δ,δ)`. On
`B_s = {‖δ‖ ≤ s}`: `‖T(δ)‖ ≤ η + γs² ≤ s` for `s ∈ [r, r₊]`, and since
`q(δ,δ) − q(δ',δ') = q(δ − δ', δ) + q(δ', δ − δ')`, `‖T(δ) − T(δ')‖ ≤ 2γs‖δ − δ'‖`, a
contraction for `s < 1/(2γ)`. With `4γη < 1`, `r < 1/(2γ)`, so Banach's theorem on
`B_r` gives existence of a zero with `‖δ‖ ≤ r`; for any `s ∈ [r, 1/(2γ))` it is the only
one in `B_s`, hence the only one with `‖δ‖ < 1/(2γ)`. ∎

Applied with `γ = β/μ` (`‖J⁻¹q(x,y)‖ ≤ ‖q(x,y)‖/μ`). The Rust validates the root
instead of trusting the formula: `r` is computed rounded up and accepted only if
`η + γr² ≤ r` and `2γr < 1` hold with directed rounding; `uniqueness_radius = 1/(2γ)`
rounded down. With `β = 0` (no axial-force-dependent terms) it reduces to the linear
bound `η`.

**Stability of the exact state.** If `λ_min(sym(K + K_G(ũ))_FF) ≥ μ_A` (Gram +
Gershgorin), then `λ_min(sym(K + K_G(u*))) ≥ μ_A − βr`, because
`|xᵀ(K_G(u*) − K_G(ũ))x| = |Σ_e c_e(t_eᵀδ) x_eᵀG_e x_e| ≤ ‖δ‖ Σ_e w_e ‖x_e‖² ≤ βr‖x‖²`.
When positive it is reported as `stabilityMargin`: the exact second-order state is
stable (`K + K_G(u*) ≻ 0`), independently of the solver's Cholesky/LU path.

**Output.** `absBound = r` on `‖u*_F − ũ_F‖₂`, `relBound`, `uniquenessRadius`, `mu`,
`beta`, `residualNorm` (`≥ ‖F(ũ)‖₂`), `newtonStep` (`η`), `stabilityMargin`. The
solver's iteration count does not matter: an iteration-limited answer is certified with
a bound on its distance to the true fixed point (tests: 2 iterations at 0.8·P_cr, true
error 2.24e-8, bound 2.26e-8). Past the critical load the reversed-sway LU answer is
never certified stable. Uncertified when `sym(J)` is not certified positive definite
(e.g. a candidate near an unstable equilibrium) or `4γη < 1` fails (candidate too far
for the curvature bound).

Honesty test (`tests/certify_nonlinear.rs`): the exact fixed point of the same model by
Newton's method with double-double residuals. Not formalized in Lean.

## Response spectrum (SRSS / CQC)

`engine/src/certify/spectral.rs` (wasm: `certify_spectral_2d/3d`).

**What is certified.** The spectral response, combined by the solver's rule (SRSS, or
CQC with the solver's correlation coefficients), of the **exact lowest `m` modes** of the
pencil `(K, M)` — `m` the number of modes in the input — with the spectrum evaluated at
the exact periods. `K` and `M` are the f64 sparse assemblies (as in "Modal enclosures"),
the influence vector `r` the solver's (unit translation in the direction), `Sa(T)` the
piecewise-linear interpolation of the spectrum points (constant beyond the ends), times
`9.81` (when in g, the f64 constant) `× I / R` (exact real quotient). Nothing the input
says about ω, periods, participation factors or effective masses is trusted; the mode
shapes and ω are candidates. Mass-participation truncation is not accounted for: the
certificate is for the response of exactly the lowest `m` modes (that they *are* the
lowest `m`, with none missing, is part of the certificate). Constraints, inclined
supports, curved beams and warping DOFs are refused.

**Modal response through the eigenprojector.** With `C = K^{-1/2}MK^{-1/2}` (eigenvalues
`ν = 1/λ`), `y = K^{1/2}φ` and `b = K^{-1/2}Mr`, for an exact simple eigenpair

    u_i* = φ (φᵀMr)/(φᵀMφ) · Sa(Tᵢ)/ωᵢ² = K^{-1/2} Πᵢ b · Sa(Tᵢ),     Πᵢ = yyᵀ/‖y‖²

(`φᵀMφ = νᵢ‖y‖²`, `λᵢνᵢ = 1`), independent of the normalization and sign of `φ`. For the
computed `φ̃` (projector `Π̃` of `ỹ = K^{1/2}φ̃`), `K^{-1/2}Π̃b = φ̃ (φ̃ᵀMr)/(φ̃ᵀKφ̃)`, so

    ‖u_i* − αᵢ φ̃‖₂ ≤ eᵢ := sinθᵢ · ‖Mr‖₂ · |Sa(Tᵢ)| / σ_K,    αᵢ = (φ̃ᵀMr)/(φ̃ᵀKφ̃) · Sa(Tᵢ),

using `‖K^{-1/2}‖₂ ≤ σ_K^{-1/2}`, `‖b‖₂ ≤ ‖Mr‖₂ σ_K^{-1/2}`, `‖Πᵢ − Π̃‖₂ = sinθᵢ`
(rank-one orthogonal projectors). `αᵢ` is an interval (`Sa` over the period enclosure).

**Isolation and eigenvector error (Davis–Kahan, single vector).** The modes are sorted by
ω̃² (strictly increasing; repeated frequencies are refused — the modal response of a
multiple eigenvalue depends on the basis, so the SRSS/CQC of "the exact modes" is not
defined). Cutoffs `0 = μ₀ < μ₁ < … < μ_m` are placed in the gaps (midpoint in ν; the
last one at `(1 + t)λ̃_m`, the largest `t ∈ {0.5, …, 0.001}` that certifies) and at each
`μ_j` the two completeness certificates of "Modal enclosures" give `#{λ < μ_j} ≥ j` and
`#{λ ≤ μ_j} ≤ j`. Hence exactly one eigenvalue `λ_j` (simple) lies in `(μ_{j−1}, μ_j)`
and every other eigenvalue `ν_k` of `C` (including the `ν = 0` of a singular `M`) is
`≥ 1/μ_{j−1}` or `≤ 1/μ_j`. With `ρⱼ ≥ ‖Cỹ − ν̂ỹ‖/‖ỹ‖` (as in the single-mode
enclosure) and `δⱼ = min(1/μ_{j−1} − ν̂, ν̂ − 1/μ_j)`:

    sinθⱼ ≤ min(1, ρⱼ / δⱼ).

*Proof.* Write the unit `ỹ = c y + s z`, `y` the eigenvector, `z ⊥ y` unit. Projecting the
residual onto `y^⊥` (which `C` leaves invariant) gives `s(C − ν̂)z`, and
`‖(C − ν̂)z‖ ≥ δ` because `z` lies in the span of the other eigenvectors. So
`|s|δ ≤ ‖Cỹ − ν̂ỹ‖ ≤ ρ`. ∎ The eigenvalue enclosure is the bracket, tightened by the
Bauer–Fike ball `[ν̂ − ρ, ν̂ + ρ]` when that ball lies inside the bracket's ν-range (then
the eigenvalue it contains is `λ_j`).

**Spectrum on period intervals.** `Tⱼ = 2π/√λⱼ ∈ [T_lo, T_hi]` (π enclosed by one ulp).
Since `Sa` is continuous and piecewise linear, its range over `[T_lo, T_hi]` is the hull
of its values at `T_lo`, `T_hi` (interval-evaluated interpolation) and at the breakpoints
inside; the spectrum periods must be finite and strictly increasing (else refused).

**Combination.** Per free DOF, `u_ik* ∈ αᵢφ̃_k + [−eᵢ, eᵢ]`; SRSS `√(Σᵢ u_ik²)` and CQC
`√|Σᵢⱼ u_ik ρᵢⱼ u_jk|` are evaluated in interval arithmetic, with
`ρᵢⱼ = 8ξ²(1+r)r^{3/2} / ((1−r²)² + 4ξ²r(1+r)²)`, `r = ωⱼ/ωᵢ` over the ω enclosures
(`ρᵢᵢ = 1` exactly; refused if a denominator enclosure contains 0).

* **Member forces**: per mode the solver's force recovery at `ũᵢ = mid(αᵢ)φ̃`, the error
  bound `bᵢ = eᵢ + ‖αᵢφ̃ − ũᵢ‖` fed to the member force bounds (`max(|start|, |end|)` is
  1-Lipschitz), then SRSS/CQC in intervals. As for member force bounds, rounding in the
  f64 force recovery is not included. The recovery is from the modal displacements
  alone, with no element loads: static member loads in the model (distributed, point,
  thermal) are not part of any mode's response, so their fixed-end forces enter neither
  the solver's modal forces nor the certified ones. (Before this was fixed the solver's
  modal recovery added those fixed-end forces to every mode, and SRSS/CQC combined them
  as if dynamic; `validation_spectral_element_loads_do_not_leak` and its 3D twin pin the
  loaded and unloaded models to the same spectral member forces.)
* **Base shear** (the solver always combines it by SRSS): `meffᵢ* = λᵢ‖Πᵢb‖²` with
  `‖Πᵢb‖ ∈ |φ̃ᵀMr|/√(φ̃ᵀKφ̃) ± sinθᵢ‖Mr‖/√σ_K`, times `Sa(Tᵢ)`, SRSS in intervals.

**Output.** Per mode: ω², period and `Sa` enclosures, `sinθ`, `eᵢ`; per node and member
the enclosures of the combined values; the base-shear enclosure; bounds on
`|exact − reported|` (max over displacement components, over force components, and for
the base shear) and whether every reported value lies inside its enclosure. A wrong
participation factor or ω in the input, or a tampered result, shows up as
`reportedWithin = false` (the enclosures do not use them); a missing mode makes the
isolation fail (`Uncertified`).

Honesty test (`tests/certify_nonlinear.rs`): an independent dense reference (Cholesky of
`K`, cyclic Jacobi on `L⁻¹ML⁻ᵀ`) of the exact modal response, combined in f64; the
reference lies inside every enclosure (to 1e-14 relative, its own accuracy). Not
formalized in Lean.

## Harmonic (steady-state) response

`engine/src/certify/harmonic.rs` (wasm: `certify_harmonic_2d/3d`).

**Model.** At each reported frequency, `ω` is the solver's f64 `2π·f` (a mismatch with
the reported `omega` is refused) and the exact steady state solves

    A u = f,    A = K_d + iωC,    K_d = K − ω²M,    C = a₀M + a₁K,

with `K`, `M` the f64 sparse assemblies on the free DOFs, `f` the assembled load, and
`(a₀, a₁)` the Rayleigh coefficients the solver derived (returned by
`solve_harmonic_{2d,3d}_with_damping`; reported in the certificate). This is the full
(untruncated) Rayleigh-damped system; the solver's modal superposition with a filtered
set of modes plus a static correction of the omitted ones (below) is one approximation of
it. Constraints and inclined supports are refused.

**Lemma (rotated coercivity).** For `s ≥ 0` let `B(s) = K_d + sωC =
(1 + sωa₁)K − (ω² − sωa₀)M` (real symmetric). If `λ_min(B(s)) ≥ μ_s > 0` then for every
complex `x`, `‖Ax‖₂ ≥ μ‖x‖₂` with `μ = μ_s/√(1 + s²)`; so `A` is invertible and for any
`ũ`, `‖u* − ũ‖₂ ≤ ‖f − Aũ‖₂/μ`.

*Proof.* `K`, `M` are real symmetric, so `a = xᴴK_dx` and `c = xᴴCx` are real and
`xᴴAx = a + iωc`. Then `Re((1 − is)xᴴAx) = a + sωc = xᴴB(s)x`, and for `x = p + iq`,
`xᴴB(s)x = pᵀBp + qᵀBq ≥ μ_s‖x‖²`. Hence `√(1+s²)·‖x‖‖Ax‖ ≥ |1 − is|·|xᴴAx| ≥ μ_s‖x‖²`. ∎

`s = 0` is plain coercivity of `K − ω²M` (below the first resonance). Above it, damping
supplies it: in modal terms `B(s) ≻ 0` iff `s > (ω² − λⱼ)/(2ξⱼωⱼω)` for every mode
below `ω`, so with `C ≻ 0` (`a₁ > 0`) some `s` always works at any `ω` — including at
resonance. `B(s)` is enclosed entrywise (`1 + sωa₁` and `ω² − sωa₀` in intervals) and
certified with the Gram residual + Gershgorin check on the real n×n matrix; no 2n×2n
system and no normal equations `AᴴA` are formed, so there is no size cap. `s` is tried
from `{previous point's s, 0, 0.1, 0.3, 1, …, 10⁵}`; soundness does not depend on it.
An undamped system above its first resonance (`C = 0`, `K_d` indefinite) is refused: its
invertibility would need a σ_min certificate of an indefinite matrix, not implemented.

**Candidate and residual.** The solver reports only the target DOF, so the certificate
computes its own full `ũ`: with `H = B(s)` (sparse Cholesky) and `G = ωC − sK_d`,
`(1 − is)A = H + iG`; CG in the H-inner product on `(I + W²)u = (I − iW)H⁻¹(1 − is)f`,
`W = H⁻¹G` (`I + W²` is H-self-adjoint, `⪰ I`), plus two steps of refinement. Nothing
about it is trusted: `r = f − Aũ` (real and imaginary parts) is enclosed in interval
arithmetic and `absBound = ‖r‖₂/μ` rounded up.

**Output.** Per frequency: `absBound` (whole free vector), `mu`, `shift` (`s`), and
enclosures `ũ_t ± absBound` of the exact real part, imaginary part and amplitude at the
target DOF; `reportedError = absBound + |ũ_t − reported_t|` bounds `|u*_t − reported_t|`,
and therefore includes the solver's modal-truncation error. The exact
peak amplitude over the sampled frequencies is enclosed by the max of the per-point
enclosures (the max is 1-Lipschitz). The phase is not bounded.

Honesty tests (`tests/certify_nonlinear.rs`): the same system solved by LU of the real
2n×2n block matrix with double-double residual refinement, at frequencies below, near,
at and above the first resonance; the reference lies within every bound. Unit tests use
decoupled systems with closed-form solutions. Not formalized in Lean.

**What the certificate found: modal truncation.** The solver superposes the lowest modes
(at most `min(100, n/2)` from the eigensolve, and only those with `ω_j < 4·ω_max`); plain
superposition left the reported values off by up to 7e-5 of the peak on the 18-DOF test
frame (reported peak 6.258509e-2 vs. the certified exact 6.258518e-2, max reported error
4.35e-6). The solver now adds the omitted modes' contribution as a series in
`z/(λd)` (`d = 1 + ia₁ω`, `z = ω² − ia₀ω`, both exact for Rayleigh damping): the static
(mode-acceleration) term `r₁/d` with `r₁ = (K⁻¹f)ₜ − Σ_kept pⱼ/λⱼ` and its first dynamic
term `z r₂/d²` with `r₂ = (K⁻¹MK⁻¹f)ₜ − Σ_kept pⱼ/λⱼ²` (two solves with the factored K),
applied at a frequency when `|z| < ½ λ_omit|d|` for a lower bound `λ_omit` on the omitted
eigenvalues. On the same frame the reported peak is 6.258518e-2 and the max reported
error 5.06e-9 (still `reportedWithin = false`: the enclosures are ~1e-12 wide and the
remaining error is the series' `O(q²)` tail and the eigensolver's accuracy). Max error
over the sweep, relative to the peak: 6.95e-5 → 8.1e-8 (18 DOF, dense path), 5.0e-5 →
9.5e-10 (72 DOF, sparse path) (`harmonic_static_correction_matches_exact`). The result
reports `method` (`modal`, `modal+static`, `direct`), `modesUsed`, `nDof`,
`truncationRatio` (share of the static target response carried by the omitted modes) and
`warnings` (frequencies where the correction could not be applied,
`harmonic_truncation_is_reported`).

## Rounding model

f64 ops round to nearest, so the exact result is within 0.5 ulp of the computed one.
Outward rounding = compute in round-to-nearest, then `next_down` the lower endpoint
and `next_up` the upper. Non-finite endpoints abort the certificate. In Lean this is
modelled by abstract `down, up : ℝ → ℝ` with `down x ≤ x ≤ up x`.

## Out of scope for v1

Corotational / large-displacement nonlinear static solvers (their residuals involve
rotations and are not polynomial in `u`; the quadratic Kantorovich argument of "P-Delta"
does not apply as is), material nonlinearity, time integration. Mechanism certificates (exact rational null vectors), and proving the f64 implementation matches the Lean algorithm. (Large
models are covered by the Gram/Gershgorin certificate above; the dense interval
Cholesky is used only below the solver's sparse threshold.)
