# Engineering logic in TypeScript

`web/src/lib/engine` holds ~333 non-test TypeScript modules (~108k lines). The Rust engine
(`engine/`) is where the verification lives: certificates, Lean, Kani, fuzzing, mutation
testing. Anything that computes an engineering result in TypeScript sits outside that
verification. This document lists every module, says which class it is in, and records the
plan and its status for bringing the safety-relevant TypeScript under verification.

Classes:

- **(a) delegates to WASM.** The number comes from a Rust/WASM call (`wasm-solver.ts`, a
  `*-kernel.ts` registry). This is the target pattern (`design/other-codes/steel-codes.ts`,
  `cfs-code.ts`, `codes/argentina/rc-section-kernel.ts`).
- **(b) duplicates a Rust check.** The same quantity is computed in TypeScript and in
  `engine/src`. Two implementations of one rule drift apart. These get differential tests
  first, then migrate.
- **(c) TypeScript-only engineering arithmetic.** Loads, code coefficients, capacities,
  model transforms with no Rust counterpart. These get property tests and mutation testing,
  and the load generators are migration candidates later.
- **(d) search/presentation.** Candidate search, orchestration, drawings, SVG, reports,
  types. Whatever they produce is checked again by (a), (b) or (c), and the re-check gate
  below makes sure the engine has the final word on a pass.

## Summary

In progress.

## Findings from the inventory

These came from reading the modules. Each one is either fixed on this branch or listed
under "What is left".

1. **CIRSOC 301 steel is only ever checked in TypeScript.** `verification-service.ts` runs
   `codes/argentina/cirsoc301.ts`, and so does `steel/profile-optimise.ts`. No Rust checker
   implements CIRSOC 301. The Rust `steel_check.rs` is AISC 360, a different code with
   different factors (φc = 0,90 against 0,85; no 1,5·My cap on Mp; 1,6·My weak-axis cap
   against 1,5).
2. **Three WASM checkers had no callers.** `checkCirsoc201Members`,
   `checkServiceability` and `checkSpreadFootings` are wrapped in `wasm-solver.ts`, and no
   app code calls them. The app runs the TypeScript duplicates instead.
3. **The WASM design-check wrappers swallowed errors.** `wasm-solver.ts` (`checkSteelMembers`
   … `checkSpreadFootings`) turned every exception into `null`. Callers have to treat `null`
   as "not checked", never as a pass. `design/other-codes/run.ts` does that. A new caller
   must do the same.
4. **The station extraction falls back to JS without saying so.** `verification-service.ts`
   `computeStationDemands` uses the JS `extractElementStations` when WASM is not ready.
5. **The Rust CIRSOC 201 check does not follow CIRSOC 201.** `cirsoc201_check.rs` used
   Vc = 0,17·√f'c·bw·d whether or not stirrups are present. For a beam below Av,min,
   CIRSOC 201-2025 Tabla 22.5.5.1(c) gives 0,66·λs·ρw^⅓·√f'c, which is about 30 % lower for
   an ordinary beam. It also fixed εty = 0,002 for every fy (21.2.2.1 permits that only for
   fy = 420 MPa), and its compression-steel branch ignored strain compatibility.
6. **φ in the TypeScript verifier uses the 2005 tension-controlled limit.**
   `station-design-forces.ts` `computeFlexureCapacity` and `cirsoc201-basis.ts`
   `phiFromStrain` use εt = 0,005. CIRSOC 201-2025 Tabla 21.2.2 uses εty + 0,003. At
   fy = 420 MPa the TypeScript ramp stays below the permitted εty = 0,002 ramp, so it is
   conservative. At fy = 500 MPa it overstates φ by up to 5 %.
7. **auto-verify gave tension members compression credit in shear.** `auto-verify.ts`
   passes `Nu = max(|N compression|, |N tension|)` to `verifyElement`, whose convention is
   "+ = compression". A member in tension got the compression term (1 + Nu/(14·Ag)) on Vc.
8. **"warn" meant different things.** In CIRSOC 201 a warn is 0,9 < ratio ≤ 1,0, still a
   pass. In `cirsoc301.ts` a warn was 1,00 < ratio ≤ 1,05, and in
   `design-check-results.ts` (`ratioToStatus`, used for every WASM checker) it was
   1,0 < ratio ≤ 1,1. Those members fail the code and were shown in amber.
9. **The crack-width estimate used the inch-pound coefficient.** `serviceability.ts`
   `checkCrackWidth` (only the report uses it) computed Gergely–Lutz with 0,076 and SI
   inputs. The SI coefficient is 0,011, so it reported cracks 6,9 times too wide.
10. **`spectral-case.ts` duplicates SRSS/CQC** from `solver/spectral.rs`.

## Plan and status

Each section below is filled in as the work lands. The
PR body holds the live checklist.

### 1. Inventory: done (this file)

### 2. Differential tests, TypeScript against Rust

In progress.

### 3. Engine re-check gate

In progress.

### 4. Migration of the duplicates into Rust

In progress.

### 5. Hardening what stays in TypeScript

In progress.

## What is left

In progress.

## Inventory

In progress.
