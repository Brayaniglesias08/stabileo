/-
Exhaustive axiom audit of the `Certify` library, run by `scripts/check-axioms.sh` with
`lake env lean scripts/AxiomScan.lean` after `lake build`.

For every constant declared in a `Certify.*` module (theorems, lemmas and the executable
check functions alike) it collects the axioms the constant depends on and fails on any axiom
outside Lean's standard three (`propext`, `Classical.choice`, `Quot.sound`). This catches
`sorry` (`sorryAx`), `native_decide` (in Lean 4.34 an auxiliary `axiom …native_decide.ax_*`; earlier
`Lean.ofReduceBool`) and any
`axiom` declaration, whether or not the dependent theorem is in the script's named list.

It also fails on any `axiom` declared in `Certify`, and on `@[implemented_by]` / `@[extern]`
in `Certify`: those swap the compiled code that `certcheck` runs for code the kernel never
saw, so the proofs about `check` would no longer be about what runs.
-/
import Certify

open Lean Elab Command

run_cmd do
  let env ← getEnv
  let allowed : Array Name := #[``propext, ``Classical.choice, ``Quot.sound]
  let mut problems : Array String := #[]
  let mut scanned := 0
  let mut theorems := 0
  for (name, ci) in env.constants.map₁.toList do
    let some idx := env.getModuleIdxFor? name | continue
    let some mod := env.header.moduleNames[idx.toNat]? | continue
    unless (`Certify).isPrefixOf mod do continue
    scanned := scanned + 1
    if ci matches .thmInfo _ then theorems := theorems + 1
    if ci matches .axiomInfo _ then
      problems := problems.push s!"{mod}: axiom declaration {name}"
    if (Compiler.implementedByAttr.getParam? env name).isSome then
      problems := problems.push s!"{mod}: @[implemented_by] on {name}"
    if isExtern env name then
      problems := problems.push s!"{mod}: @[extern] on {name}"
    let axs ← liftCoreM <| collectAxioms name
    for ax in axs do
      unless allowed.contains ax do
        problems := problems.push s!"{mod}: {name} depends on axiom {ax}"
  if scanned == 0 then
    throwError "axiom scan: no Certify constants found (wrong import?)"
  if problems.isEmpty then
    logInfo m!"axiom scan ok: {scanned} Certify constants ({theorems} theorems), \
      axioms ⊆ [propext, Classical.choice, Quot.sound], no implemented_by/extern"
  else
    throwError m!"axiom scan failed:\n{String.intercalate "\n" problems.toList}"
