/-
Soundness of the executable bundle checker (`Certify.BundleCheck`).

`check d = true` implies the conclusion of `certified_error_bound_gram` for the real
matrices the bundle denotes (`kMat d`, `lMat d`, `uVec d`) and every right-hand side `f`
inside the bundle's enclosure: `K` is invertible and `‖K⁻¹f − ũ‖₂ ≤ bound`.

The proof instantiates `certified_error_bound_gram` with point intervals (the residual
`R = K − L Lᵀ` is computed exactly, so its enclosure is `[Rᵢⱼ, Rᵢⱼ]`), after showing that
the hash-map computations of `R`, `Σ_{j≠i}|Rᵢⱼ|` and `K ũ` equal the mathematical sums.
-/
import Certify.Gram
import Certify.BundleCheck

open Matrix Finset

namespace Certify.Bundle

open Certify Certify.Interval

/-! ### Denotation -/

/-- Contribution of entry `e = (a, b, v)` of `kEnt` to `K i j`. -/
def kTerm (i j : ℕ) (e : ℕ × ℕ × ℚ) : ℚ :=
  (if (e.1, e.2.1) = (i, j) then e.2.2 else 0) +
    (if e.1 = e.2.1 then 0 else if (e.2.1, e.1) = (i, j) then e.2.2 else 0)

/-- Entry `(i, j)` of the symmetric `K` denoted by the bundle. -/
def kAt (d : Bundle) (i j : ℕ) : ℚ := (d.kEnt.map (kTerm i j)).sum

/-- Entry `(i, c)` of `L'` (original row numbering): `Σ v` over the entries `(a, v)` of
column `c` with `perm[a] = i`. -/
def colAt (perm : Array ℕ) (i : ℕ) (col : List (ℕ × ℚ)) : ℚ :=
  (col.map fun p => if perm.getD p.1 0 = i then p.2 else 0).sum

def lAt (d : Bundle) (i c : ℕ) : ℚ := colAt d.perm i (d.lCols.getD c [])

/-- The real matrix `K` denoted by the bundle. -/
noncomputable def kMat (d : Bundle) : Matrix (Fin d.n) (Fin d.n) ℝ := fun i j => (kAt d i j : ℝ)

/-- The real factor `L'` (rows un-permuted) denoted by the bundle. -/
noncomputable def lMat (d : Bundle) : Matrix (Fin d.n) (Fin d.n) ℝ := fun i c => (lAt d i c : ℝ)

/-- The computed solution `ũ`. -/
noncomputable def uVec (d : Bundle) : Fin d.n → ℝ := fun i => (d.u.getD i 0 : ℝ)

/-- The enclosure of the right-hand side. -/
noncomputable def fLoVec (d : Bundle) : Fin d.n → ℝ := fun i => (d.fLo.getD i 0 : ℝ)
noncomputable def fHiVec (d : Bundle) : Fin d.n → ℝ := fun i => (d.fHi.getD i 0 : ℝ)

/-! ### Hash-map accumulation -/

section Accum

variable {κ α : Type} [BEq κ] [Hashable κ]

lemma getD_addTo [LawfulBEq κ] (m : Std.HashMap κ ℚ) (k a : κ) (v : ℚ) :
    (addTo m k v).getD a 0 = m.getD a 0 + if k == a then v else 0 := by
  unfold addTo
  rw [Std.HashMap.getD_insert]
  by_cases h : k = a
  · subst h; simp
  · simp [h]

/-- A left fold whose steps each add `c x` at key `k` adds `Σ c x` at `k`. -/
lemma foldl_getD (F : Std.HashMap κ ℚ → α → Std.HashMap κ ℚ) (c : α → ℚ) (k : κ)
    (hF : ∀ m x, (F m x).getD k 0 = m.getD k 0 + c x) :
    ∀ (l : List α) (m : Std.HashMap κ ℚ), (l.foldl F m).getD k 0 = m.getD k 0 + (l.map c).sum
  | [], m => by simp
  | x :: l, m => by
    rw [List.foldl_cons, foldl_getD F c k hF l, hF, List.map_cons, List.sum_cons]; ring

end Accum

lemma getD_addK (m : Std.HashMap (ℕ × ℕ) ℚ) (e : ℕ × ℕ × ℚ) (i j : ℕ) :
    (addK m e).getD (i, j) 0 = m.getD (i, j) 0 + kTerm i j e := by
  unfold addK kTerm
  by_cases h : e.1 = e.2.1
  · simp only [h, ite_true, getD_addTo, beq_iff_eq]; simp
  · simp only [h, ite_false, getD_addTo, beq_iff_eq]; ring

lemma getD_addLCol (perm : Array ℕ) (m : Std.HashMap (ℕ × ℕ) ℚ) (col : List (ℕ × ℚ)) (i j : ℕ) :
    (addLCol perm m col).getD (i, j) 0 = m.getD (i, j) 0 - colAt perm i col * colAt perm j col := by
  unfold addLCol
  rw [foldl_getD _ (fun p => (if perm.getD p.1 0 = i then p.2 else 0) * -colAt perm j col) (i, j)]
  · rw [List.sum_map_mul_right]; simp only [colAt]; ring
  · intro m p
    rw [foldl_getD _ (fun q : ℕ × ℚ => if (perm.getD p.1 0, perm.getD q.1 0) == (i, j) then -(p.2 * q.2) else 0)
      (i, j) (fun m q => getD_addTo _ _ _ _)]
    congr 1
    rw [colAt, mul_neg, ← List.sum_map_mul_left, ← neg_one_mul, ← List.sum_map_mul_left]
    congr 1
    refine List.map_congr_left fun q _ => ?_
    generalize perm.getD p.1 0 = x
    generalize perm.getD q.1 0 = y
    by_cases hi : x = i <;> by_cases hj : y = j <;> simp [hi, hj]

lemma sum_map_neg' {β : Type} (l : List β) (g : β → ℚ) : (l.map fun x => -g x).sum = -(l.map g).sum := by
  induction l with
  | nil => simp
  | cons x l ih => simp only [List.map_cons, List.sum_cons, ih]; ring

lemma residualMap_getD (d : Bundle) (i j : ℕ) :
    (residualMap d).getD (i, j) 0 =
      kAt d i j - (d.lCols.toList.map fun col => colAt d.perm i col * colAt d.perm j col).sum := by
  unfold residualMap
  rw [← Array.foldl_toList, foldl_getD _ (fun col => -(colAt d.perm i col * colAt d.perm j col)) (i, j)
    (fun m col => by rw [getD_addLCol]; ring), foldl_getD _ (kTerm i j) (i, j)
    (fun m e => getD_addK m e i j)]
  simp [kAt, sum_map_neg', sub_eq_add_neg]

/-- A sum over `Fin n` of an array of size `n` is the sum over its list. -/
lemma sum_fin_getD {β : Type} {n : ℕ} (arr : Array β) (dflt : β) (g : β → ℚ) (h : arr.size = n) :
    ∑ c : Fin n, g (arr.getD c dflt) = (arr.toList.map g).sum := by
  subst h
  rw [← List.sum_ofFn]
  congr 1
  rw [← Array.toList_map, ← Array.ofFn_getElem (xs := arr.map g)]
  simp [Array.getD]

/-! ### `K` is symmetric; the real residual -/

lemma kTerm_symm (i j : ℕ) (e : ℕ × ℕ × ℚ) : kTerm i j e = kTerm j i e := by
  obtain ⟨a, b, v⟩ := e
  simp only [kTerm, Prod.mk.injEq]
  by_cases hab : a = b
  · subst hab
    by_cases h1 : a = i <;> by_cases h2 : a = j <;> simp_all
  · by_cases h1 : a = i <;> by_cases h2 : b = j <;> by_cases h3 : a = j <;> by_cases h4 : b = i <;>
      simp_all

lemma kAt_symm (d : Bundle) (i j : ℕ) : kAt d i j = kAt d j i := by
  unfold kAt
  congr 1
  exact List.map_congr_left fun e _ => kTerm_symm i j e

/-- The exact residual `symPart K − L' L'ᵀ` is what `residualMap` stores. -/
lemma resid_eq (d : Bundle) (h : d.lCols.size = d.n) (i j : Fin d.n) :
    (symPart (kMat d) - lMat d * (lMat d)ᵀ) i j = ((residualMap d).getD (i.val, j.val) 0 : ℝ) := by
  rw [Matrix.sub_apply, symPart_apply, Matrix.mul_apply, residualMap_getD,
    ← sum_fin_getD d.lCols [] (fun col => colAt d.perm i col * colAt d.perm j col) h]
  simp only [kMat, lMat, lAt, Matrix.transpose_apply]
  rw [kAt_symm d j i]
  push_cast
  ring

/-! ### Off-diagonal sums -/

lemma qabs_eq (x : ℚ) : qabs x = |x| := by
  unfold qabs
  split_ifs with h
  · exact (abs_of_neg h).symm
  · exact (abs_of_nonneg (not_lt.mp h)).symm

lemma qmax_eq (a b : ℚ) : qmax a b = max a b := by
  unfold qmax
  split_ifs with h
  · exact (max_eq_right h).symm
  · exact (max_eq_left (le_of_lt (not_le.mp h))).symm

/-- Contribution of a stored entry of `R` to the off-diagonal sum of row `i`. -/
def offTerm (i : ℕ) (e : (ℕ × ℕ) × ℚ) : ℚ := if e.1.1 = e.1.2 then 0 else if e.1.1 = i then qabs e.2 else 0

lemma offTerm_nonneg (i : ℕ) (e : (ℕ × ℕ) × ℚ) : 0 ≤ offTerm i e := by
  unfold offTerm
  split_ifs <;> simp [qabs_eq]

lemma offSums_getD (R : Std.HashMap (ℕ × ℕ) ℚ) (i : ℕ) :
    (offSums R).getD i 0 = (R.toList.map (offTerm i)).sum := by
  unfold offSums
  rw [Std.HashMap.fold_eq_foldl_toList, foldl_getD _ (offTerm i) i]
  · simp
  · intro m e
    unfold offTerm
    split_ifs with h1 h2 <;> simp_all [getD_addTo]

lemma sum_toFinset_le {β : Type} [DecidableEq β] (F : β → ℚ) (hF : ∀ x, 0 ≤ F x) :
    ∀ l : List β, ∑ x ∈ l.toFinset, F x ≤ (l.map F).sum
  | [] => by simp
  | a :: l => by
    rw [List.toFinset_cons, List.map_cons, List.sum_cons]
    have ih := sum_toFinset_le F hF l
    by_cases ha : a ∈ l.toFinset
    · rw [Finset.insert_eq_of_mem ha]; linarith [hF a]
    · rw [Finset.sum_insert ha]; linarith

lemma mem_toList_of_getD_ne (R : Std.HashMap (ℕ × ℕ) ℚ) (k : ℕ × ℕ) (h : R.getD k 0 ≠ 0) :
    (k, R.getD k 0) ∈ R.toList := by
  rw [Std.HashMap.mem_toList_iff_getElem?_eq_some]
  rw [Std.HashMap.getD_eq_getD_getElem?] at h ⊢
  cases hk : R[k]? with
  | none => rw [hk] at h; simp at h
  | some v => simp

/-- The computed off-diagonal sum bounds the true one (the stored entries cover every
nonzero `Rᵢⱼ`; the hash map's entries are distinct keys, so this is in fact equality). -/
lemma off_le (R : Std.HashMap (ℕ × ℕ) ℚ) {n : ℕ} (i : Fin n) :
    ∑ j ∈ univ.erase i, |R.getD (i.val, j.val) 0| ≤ (offSums R).getD i 0 := by
  classical
  rw [offSums_getD]
  set g : Fin n → ℚ := fun j => |R.getD (i.val, j.val) 0|
  set emb : Fin n → (ℕ × ℕ) × ℚ := fun j => ((i.val, j.val), R.getD (i.val, j.val) 0)
  have hinj : ∀ a b : Fin n, emb a = emb b → a = b := by
    intro a b hab
    simp only [emb, Prod.mk.injEq] at hab
    exact Fin.ext hab.1.2
  have h1 : ∑ j ∈ univ.erase i, g j = ∑ j ∈ (univ.erase i).filter (fun j => g j ≠ 0), g j :=
    (Finset.sum_filter_ne_zero _).symm
  have h2 : ∑ j ∈ (univ.erase i).filter (fun j => g j ≠ 0), g j =
      ∑ e ∈ ((univ.erase i).filter (fun j => g j ≠ 0)).image emb, offTerm i e := by
    rw [Finset.sum_image fun a _ b _ hab => hinj a b hab]
    refine Finset.sum_congr rfl fun j hj => ?_
    have hji : j ≠ i := Finset.ne_of_mem_erase (Finset.mem_filter.mp hj).1
    have hv : i.val ≠ j.val := fun h => hji (Fin.ext h).symm
    simp [offTerm, emb, g, hv, qabs_eq]
  have h3 : ((univ.erase i).filter (fun j => g j ≠ 0)).image emb ⊆ R.toList.toFinset := by
    intro e he
    obtain ⟨j, hj, rfl⟩ := Finset.mem_image.mp he
    have hne : R.getD (i.val, j.val) 0 ≠ 0 := by
      have := (Finset.mem_filter.mp hj).2
      simpa [g] using this
    exact List.mem_toFinset.mpr (mem_toList_of_getD_ne R _ hne)
  calc ∑ j ∈ univ.erase i, |R.getD (i.val, j.val) 0| = ∑ j ∈ univ.erase i, g j := rfl
    _ ≤ ∑ e ∈ R.toList.toFinset, offTerm i e := by
        rw [h1, h2]
        exact Finset.sum_le_sum_of_subset_of_nonneg h3 fun e _ _ => offTerm_nonneg i e
    _ ≤ _ := sum_toFinset_le _ (offTerm_nonneg i) _

/-! ### `K ũ` and the residual norm -/

/-- Contribution of entry `e = (a, b, v)` to `(K ũ)ᵢ`. -/
def kuTerm (u : Array ℚ) (i : ℕ) (e : ℕ × ℕ × ℚ) : ℚ :=
  (if e.1 = i then e.2.2 * u.getD e.2.1 0 else 0) +
    (if e.1 = e.2.1 then 0 else if e.2.1 = i then e.2.2 * u.getD e.1 0 else 0)

lemma kuMap_getD (d : Bundle) (i : ℕ) : (kuMap d).getD i 0 = (d.kEnt.map (kuTerm d.u i)).sum := by
  unfold kuMap
  rw [foldl_getD _ (kuTerm d.u i) i]
  · simp
  · intro m e
    unfold addKu kuTerm
    by_cases h : e.1 = e.2.1
    · simp only [h, ite_true, getD_addTo, beq_iff_eq]; simp
    · simp only [h, ite_false, getD_addTo, beq_iff_eq]; ring

lemma sum_if_val_eq {n : ℕ} (b : ℕ) (hb : b < n) (g : Fin n → ℚ) :
    ∑ j : Fin n, (if b = j.val then g j else 0) = g ⟨b, hb⟩ := by
  rw [Finset.sum_eq_single ⟨b, hb⟩]
  · simp
  · intro j _ hj
    have : b ≠ j.val := fun h => hj (Fin.ext h.symm)
    simp [this]
  · simp

lemma kTerm_mulVec {n : ℕ} (u : Array ℚ) (i : ℕ) (e : ℕ × ℕ × ℚ) (ha : e.1 < n) (hb : e.2.1 < n) :
    ∑ j : Fin n, kTerm i j e * u.getD j 0 = kuTerm u i e := by
  obtain ⟨a, b, v⟩ := e
  simp only at ha hb
  have h1 : ∀ j : Fin n, kTerm i j (a, b, v) * u.getD j 0 =
      (if b = j.val then (if a = i then v * u.getD j 0 else 0) else 0) +
      (if a = j.val then (if a = b then 0 else if b = i then v * u.getD j 0 else 0) else 0) := by
    intro j
    simp only [kTerm, Prod.mk.injEq]
    by_cases h1 : a = i <;> by_cases h2 : b = j.val <;> by_cases h3 : a = b <;> by_cases h4 : b = i <;>
      by_cases h5 : a = j.val <;> simp_all
  simp only [h1, Finset.sum_add_distrib]
  rw [sum_if_val_eq b hb (fun j => if a = i then v * u.getD j 0 else 0),
    sum_if_val_eq a ha (fun j => if a = b then 0 else if b = i then v * u.getD j 0 else 0)]
  simp [kuTerm]

lemma list_sum_finset_comm {β : Type} {n : ℕ} (l : List β) (g : Fin n → β → ℚ) :
    ∑ j : Fin n, (l.map (g j)).sum = (l.map fun e => ∑ j : Fin n, g j e).sum := by
  induction l with
  | nil => simp
  | cons x l ih => simp only [List.map_cons, List.sum_cons, Finset.sum_add_distrib, ih]

/-- `(K ũ)ᵢ` is what `kuMap` stores (all `K` indices in range). -/
lemma mulVec_eq (d : Bundle) (hk : ∀ e ∈ d.kEnt, e.1 < d.n ∧ e.2.1 < d.n) (i : Fin d.n) :
    (kMat d *ᵥ uVec d) i = ((kuMap d).getD i.val 0 : ℝ) := by
  have hq : ∑ j : Fin d.n, kAt d i j * d.u.getD j 0 = (kuMap d).getD i.val 0 := by
    rw [kuMap_getD]
    simp only [kAt, ← List.sum_map_mul_right]
    rw [list_sum_finset_comm]
    congr 1
    exact List.map_congr_left fun e he => kTerm_mulVec d.u i e (hk e he).1 (hk e he).2
  rw [← hq]
  simp [Matrix.mulVec, dotProduct, kMat, uVec]

lemma list_range_sum {n : ℕ} (f : ℕ → ℚ) : ((List.range n).map f).sum = ∑ i : Fin n, f i := by
  rw [Fin.sum_univ_eq_sum_range]
  simp [Finset.sum_eq_multiset_sum, Finset.range_val, Multiset.range]

/-- `ρ² ≤ (bμ)²` with `μ > 0`, `b ≥ 0` gives `√ρ² / μ ≤ b`. -/
lemma sqrt_div_le_of_sq_le {ρ2 μ b : ℝ} (hμ : 0 < μ) (hb : 0 ≤ b) (h : ρ2 ≤ (b * μ) ^ 2) :
    Real.sqrt ρ2 / μ ≤ b := by
  rw [div_le_iff₀ hμ]
  calc Real.sqrt ρ2 ≤ Real.sqrt ((b * μ) ^ 2) := Real.sqrt_le_sqrt h
    _ = b * μ := Real.sqrt_sq (mul_nonneg hb hμ.le)

/-! ### Soundness -/

lemma all_range {n : ℕ} {p : ℕ → Bool} (h : (List.range n).all p = true) (i : Fin n) : p i = true := by
  rw [List.all_eq_true] at h
  exact h i (List.mem_range.mpr i.isLt)

/-- **Verified checker.** If `check d = true` then, for the real matrices denoted by the
bundle (`K = kMat d`, `ũ = uVec d`) and every right-hand side `f` in the bundle's enclosure
`[fLo, fHi]`, `K` is invertible, `K⁻¹ f` is the unique solution, and
`‖K⁻¹ f − ũ‖₂ ≤ bound` — the conclusion of `certified_error_bound_gram`, to which the
proof reduces with `L = lMat d`, point enclosures of the exactly computed `R = K − L Lᵀ`,
the residual interval `[fLo − Kũ, fHi − Kũ]` and `ρ = bound · μ`. -/
theorem check_sound (d : Bundle) (h : check d = true) (f : Fin d.n → ℝ)
    (hf : ∀ i, fLoVec d i ≤ f i ∧ f i ≤ fHiVec d i) :
    kMat d *ᵥ ((kMat d)⁻¹ *ᵥ f) = f ∧ (∀ u, kMat d *ᵥ u = f → u = (kMat d)⁻¹ *ᵥ f) ∧
      vnorm ((kMat d)⁻¹ *ᵥ f - uVec d) ≤ d.bound := by
  simp only [check, shapeOk, gershgorinOk, residualOk, Bool.and_eq_true, beq_iff_eq,
    decide_eq_true_eq, List.all_eq_true, Bool.and_eq_true] at h
  obtain ⟨⟨⟨⟨⟨⟨⟨⟨⟨⟨_, _⟩, _⟩, _⟩, hlc⟩, hk⟩, _⟩, hμ0⟩, hb0⟩, hg⟩, hres⟩ := h
  have hk' : ∀ e ∈ d.kEnt, e.1 < d.n ∧ e.2.1 < d.n := fun e he => by
    simpa using hk e he
  set K := kMat d
  set L := lMat d
  set Rm := residualMap d
  -- exact residual as point intervals
  let IR : Matrix (Fin d.n) (Fin d.n) Interval := fun i j => point ((symPart K - L * Lᵀ) i j)
  have hR : ∀ i j, (symPart K - L * Lᵀ) i j ∈ᵢ IR i j := fun i j => mem_point _
  have hRe : ∀ i j, (symPart K - L * Lᵀ) i j = ((Rm.getD (i.val, j.val) 0 : ℚ) : ℝ) :=
    resid_eq d hlc
  have hμ : ∀ i, (d.mu : ℝ) ≤ (IR i i).lo - ∑ j ∈ univ.erase i, mag (IR i j) := by
    intro i
    have hrow := hg i.val (List.mem_range.mpr i.isLt)
    simp only [rowOk, decide_eq_true_eq] at hrow
    have hoff := off_le Rm i
    have hq : d.mu ≤ Rm.getD (i.val, i.val) 0 - ∑ j ∈ univ.erase i, |Rm.getD (i.val, j.val) 0| := by
      linarith
    have hr : ((d.mu : ℚ) : ℝ) ≤ ((Rm.getD (i.val, i.val) 0 - ∑ j ∈ univ.erase i,
        |Rm.getD (i.val, j.val) 0| : ℚ) : ℝ) := by exact_mod_cast hq
    simp only [IR, point, mag, max_self, hRe]
    push_cast at hr
    exact hr
  -- residual interval
  let ku : Fin d.n → ℝ := fun i => ((kuMap d).getD i.val 0 : ℝ)
  have hku : ∀ i, (K *ᵥ uVec d) i = ku i := mulVec_eq d hk'
  let Ir : Fin d.n → Interval := fun i =>
    ⟨fLoVec d i - ku i, fHiVec d i - ku i, by linarith [(hf i).1, (hf i).2]⟩
  have hr : ∀ i, (f - K *ᵥ uVec d) i ∈ᵢ Ir i := by
    intro i
    simp only [Pi.sub_apply, hku]
    exact ⟨by linarith [(hf i).1], by linarith [(hf i).2]⟩
  have hsum : ∑ i, mag (Ir i) ^ 2 = ((rho2 d : ℚ) : ℝ) := by
    rw [rho2, list_range_sum]
    push_cast
    refine Finset.sum_congr rfl fun i _ => ?_
    simp only [resTerm, qmax_eq, qabs_eq, mag, Ir, ku, fLoVec, fHiVec]
    push_cast
    ring
  have hρ2 : ∑ i, mag (Ir i) ^ 2 ≤ ((d.bound : ℝ) * d.mu) ^ 2 := by
    rw [hsum]
    have : ((rho2 d : ℚ) : ℝ) ≤ ((d.bound * d.mu * (d.bound * d.mu) : ℚ) : ℝ) := by exact_mod_cast hres
    push_cast at this
    nlinarith [this]
  have hμ0' : (0 : ℝ) < d.mu := by exact_mod_cast hμ0
  have hb0' : (0 : ℝ) ≤ d.bound := by exact_mod_cast hb0
  have hρ : Real.sqrt (∑ i, mag (Ir i) ^ 2) ≤ d.bound * d.mu :=
    (div_le_iff₀ hμ0').mp (sqrt_div_le_of_sq_le hμ0' hb0' hρ2)
  have hb : (d.bound * d.mu : ℝ) / d.mu ≤ d.bound := by
    rw [mul_div_assoc, div_self hμ0'.ne', mul_one]
  exact certified_error_bound_gram K f (uVec d) L IR hR hμ0' hμ Ir hr hρ hb

/-- The certified `μ` is a lower bound on the quadratic form of `K`: `λ_min(K) ≥ μ`. -/
theorem check_sound_coercive (d : Bundle) (h : check d = true) (x : Fin d.n → ℝ) :
    (d.mu : ℝ) * vnorm x ^ 2 ≤ x ⬝ᵥ (kMat d *ᵥ x) := by
  simp only [check, shapeOk, gershgorinOk, Bool.and_eq_true, beq_iff_eq,
    decide_eq_true_eq, List.all_eq_true] at h
  obtain ⟨⟨⟨⟨⟨⟨⟨⟨⟨⟨_, _⟩, _⟩, _⟩, hlc⟩, _⟩, _⟩, _⟩, _⟩, hg⟩, _⟩ := h
  refine coercive_of_gram (kMat d) (lMat d)
    (fun i j => point ((symPart (kMat d) - lMat d * (lMat d)ᵀ) i j)) (fun i j => mem_point _) ?_ x
  intro i
  have hrow := hg i.val (List.mem_range.mpr i.isLt)
  simp only [rowOk, decide_eq_true_eq] at hrow
  have hoff := off_le (residualMap d) i
  have hq : d.mu ≤ (residualMap d).getD (i.val, i.val) 0 -
      ∑ j ∈ univ.erase i, |(residualMap d).getD (i.val, j.val) 0| := by linarith
  have hr : ((d.mu : ℚ) : ℝ) ≤ (((residualMap d).getD (i.val, i.val) 0 - ∑ j ∈ univ.erase i,
      |(residualMap d).getD (i.val, j.val) 0| : ℚ) : ℝ) := by exact_mod_cast hq
  simp only [point, mag, max_self, resid_eq d hlc]
  push_cast at hr
  exact hr

end Certify.Bundle
