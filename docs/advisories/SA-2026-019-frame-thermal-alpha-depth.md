# SA-2026-019: frame and truss temperature loads assumed steel's α and a solid rectangle's depth

| | |
|---|---|
| Severity | Medium (engine and WASM API; the app has compensated since 2026-10-07) |
| Status | Draft (fix in review; untagged, next release) |
| Issue | [#407](https://github.com/lambdaclass/stabileo/issues/407) (FR-4 in the element-derivation review) |
| Fix | PR [#531](https://github.com/lambdaclass/stabileo/pull/531) |
| Affected versions | the engine and WASM API in every version up to 0.2.0 and `main` until the fix; the app before 2026-10-07 |
| Re-check registry | not needed for this fix: app results do not change (the app sent loads scaled to the engine's assumptions since 2026-10-07) |

## Affected features

Temperature loads (`thermal`: uniform ΔT and gradient) on frame and truss members, 2D and 3D, in
every engine analysis that takes them. Shell temperature loads already carried their own α and are not affected.

## What was wrong

The engine computed every member's thermal load with α = 12·10⁻⁶ /°C (steel's) whatever the
material, and took the depth a gradient acts across as √(12·I/A), the depth of the solid
rectangle with the section's A and I. That depth is exact only for a solid rectangle: an I
section or a tube is shallower, so its gradient moment came out too low. A different material
scaled every thermal force by α/12·10⁻⁶.

| Case | Correct | Reported before the fix |
|---|---|---|
| IPE 300 (h = 0.300 m), steel, fixed at both ends, gradient 20 °C: end moment | 14.04 kN·m | 9.76 kN·m (30 % low; depth taken as 0.432 m) |
| Aluminium bar (E = 70 GPa, A = 10 cm², α = 23·10⁻⁶), fixed at both ends, ΔT = 30 °C: axial force | 48.3 kN | 25.2 kN (48 % low) |
| Concrete member (α = 10·10⁻⁶), ΔT | αc·E·A·ΔT | 20 % high |

The app scaled each load it sent (ΔT by α/12·10⁻⁶, the gradient by √(12·I/A)/h) since
2026-10-07 (#263), so app results since then are right; before that date they had the same error.

## How to tell if your results are affected

You call the engine or its WASM API with `thermal` loads on members that are not steel, or with
a gradient on a section that is not a solid rectangle; or you solved such a model in the app
before 2026-10-07.

## Fixed in

The next release. A member thermal load takes the material's α (`alpha`) and the section's real
depth (`depth` in 2D, `depthY` / `depthZ` in 3D); the app sends both. A load without them is still
solved with the old defaults, and the result carries a warning (`thermal_default_assumed`) naming
the default, so it is never silent. The independent end-to-end certificate re-assembles thermal
loads with the same inputs.

## Workaround

Scale the load yourself: ΔT·(α / 12·10⁻⁶), and the gradient by √(12·I/A) / h.

## What to do with results you already have

Re-run API models with thermal loads on non-steel members or non-rectangular sections, and app
models with thermal loads solved before 2026-10-07.

## Credits and references

Found by the element-derivation review (FR-4, PR #330).
