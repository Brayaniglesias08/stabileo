/-
Executable checker for linear-static certificate bundles (CERTIFY.md, "Independent
checking: certificate bundles").

Everything is exact rational arithmetic (`Rat`, GMP-backed when compiled): every f64 in
the bundle is a dyadic rational `m·2^e`, so nothing is rounded and no rounding model is
needed. This module depends only on Lean core and `Std` so that `lake exe certcheck` is
compiled natively without Mathlib; `Certify.BundleSound` proves `check_sound` about the
`check` defined here.

Data, with `n` the number of unknowns:
* `kEnt` — entries `(a, b, v)` of the symmetric matrix `K`: each adds `v` at `(a, b)` and,
  if `a ≠ b`, also at `(b, a)` (the engine exports the lower triangle);
* `fLo`, `fHi` — endpoints of an enclosure of the right-hand side `f`;
* `u` — the computed solution `ũ`;
* `perm`, `lCols` — the factor: column `c` of `L` is the list `lCols[c]` of `(a, v)` in
  permuted numbering, i.e. `L'[perm[a], c] += v` in the original numbering. Nothing about
  `L` is checked or needed (`L' L'ᵀ ⪰ 0` for every real `L'`);
* `mu`, `bound` — the claimed `μ ≤ λ_min(K)` and `‖u* − ũ‖₂ ≤ bound`.

The check: `R = K − L' L'ᵀ` exactly (sparse, a hash map over the pattern), Gershgorin
`μ ≤ Rᵢᵢ − Σ_{j≠i} |Rᵢⱼ|` for every row, `r = f − K ũ` as the exact interval
`[fLo − Kũ, fHi − Kũ]`, `ρ² = Σ max(|loᵢ|, |hiᵢ|)²`, and `ρ² ≤ (bound·μ)²`.
-/
import Std.Data.HashMap

namespace Certify.Bundle

structure Bundle where
  n : Nat
  kEnt : List (Nat × Nat × Rat)
  fLo : Array Rat
  fHi : Array Rat
  u : Array Rat
  perm : Array Nat
  lCols : Array (List (Nat × Rat))
  mu : Rat
  bound : Rat

/-- `|x|` on `Rat` (core has no lattice structure). -/
def qabs (x : Rat) : Rat := if x < 0 then -x else x

def qmax (a b : Rat) : Rat := if a ≤ b then b else a

/-- Add `v` to the entry at key `k`. -/
@[inline] def addTo {κ : Type} [BEq κ] [Hashable κ] (m : Std.HashMap κ Rat) (k : κ) (v : Rat) :
    Std.HashMap κ Rat :=
  m.insert k (m.getD k 0 + v)

/-- Contribution of one `K` entry `(a, b, v)` to `R`: `+v` at `(a, b)`, and at `(b, a)` off
the diagonal. -/
@[inline] def addK (m : Std.HashMap (Nat × Nat) Rat) (e : Nat × Nat × Rat) :
    Std.HashMap (Nat × Nat) Rat :=
  let m := addTo m (e.1, e.2.1) e.2.2
  if e.1 = e.2.1 then m else addTo m (e.2.1, e.1) e.2.2

/-- Contribution of one column of `L` to `R`: `−L'ₐ L'_b` at `(perm a, perm b)` for every
pair of entries of the column (both orders). -/
@[inline] def addLCol (perm : Array Nat) (m : Std.HashMap (Nat × Nat) Rat) (col : List (Nat × Rat)) :
    Std.HashMap (Nat × Nat) Rat :=
  col.foldl (fun m p => col.foldl
    (fun m q => addTo m (perm.getD p.1 0, perm.getD q.1 0) (-(p.2 * q.2))) m) m

/-- `R = K − L' L'ᵀ`, exactly, over its sparsity pattern. -/
def residualMap (d : Bundle) : Std.HashMap (Nat × Nat) Rat :=
  d.lCols.foldl (addLCol d.perm) (d.kEnt.foldl addK ∅)

/-- `offᵢ = Σ_{j≠i} |Rᵢⱼ|` over the stored entries. -/
def offSums (R : Std.HashMap (Nat × Nat) Rat) : Std.HashMap Nat Rat :=
  R.fold (fun m k v => if k.1 = k.2 then m else addTo m k.1 (qabs v)) ∅

/-- Gershgorin row test for row `i`: `μ ≤ Rᵢᵢ − offᵢ`. -/
def rowOk (mu : Rat) (R : Std.HashMap (Nat × Nat) Rat) (off : Std.HashMap Nat Rat) (i : Nat) : Bool :=
  decide (mu ≤ R.getD (i, i) 0 - off.getD i 0)

/-- Contribution of one `K` entry to `K ũ`. -/
@[inline] def addKu (u : Array Rat) (m : Std.HashMap Nat Rat) (e : Nat × Nat × Rat) :
    Std.HashMap Nat Rat :=
  let m := addTo m e.1 (e.2.2 * u.getD e.2.1 0)
  if e.1 = e.2.1 then m else addTo m e.2.1 (e.2.2 * u.getD e.1 0)

/-- `K ũ`, exactly. -/
def kuMap (d : Bundle) : Std.HashMap Nat Rat := d.kEnt.foldl (addKu d.u) ∅

/-- `max(|loᵢ|, |hiᵢ|)²` for the residual interval `[fLoᵢ − (Kũ)ᵢ, fHiᵢ − (Kũ)ᵢ]`. -/
def resTerm (d : Bundle) (ku : Std.HashMap Nat Rat) (i : Nat) : Rat :=
  let k := ku.getD i 0
  let m := qmax (qabs (d.fLo.getD i 0 - k)) (qabs (d.fHi.getD i 0 - k))
  m * m

/-- `ρ² = Σᵢ max(|loᵢ|, |hiᵢ|)²`. -/
def rho2 (d : Bundle) : Rat :=
  let ku := kuMap d
  ((List.range d.n).map (resTerm d ku)).sum

/-- Shape checks (sizes, `K` indices in range, `fLo ≤ fHi`, `μ > 0`, `bound ≥ 0`). -/
def shapeOk (d : Bundle) : Bool :=
  d.fLo.size == d.n && d.fHi.size == d.n && d.u.size == d.n && d.perm.size == d.n &&
  d.lCols.size == d.n &&
  d.kEnt.all (fun e => decide (e.1 < d.n) && decide (e.2.1 < d.n)) &&
  (List.range d.n).all (fun i => decide (d.fLo.getD i 0 ≤ d.fHi.getD i 0)) &&
  decide (0 < d.mu) && decide (0 ≤ d.bound)

/-- Gershgorin on the exact `R` for every row. -/
def gershgorinOk (d : Bundle) : Bool :=
  let R := residualMap d
  let off := offSums R
  (List.range d.n).all (rowOk d.mu R off)

/-- `ρ² ≤ (bound·μ)²`, i.e. `ρ/μ ≤ bound` without square roots. -/
def residualOk (d : Bundle) : Bool :=
  decide (rho2 d ≤ (d.bound * d.mu) * (d.bound * d.mu))

/-- The verified checker. `Certify.Bundle.check_sound`: `check d = true` implies the
conclusion of `certified_error_bound_gram` for the matrices the data denotes. -/
def check (d : Bundle) : Bool :=
  shapeOk d && gershgorinOk d && residualOk d

end Certify.Bundle
