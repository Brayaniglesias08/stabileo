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

Mechanism certificates (exact rational null vectors), and proving the f64 implementation matches the Lean algorithm. (Large
models are covered by the Gram/Gershgorin certificate above; the dense interval
Cholesky is used only below the solver's sparse threshold.)
