# SA-2026-006: Flat 2D model solved in 3D: nodal moments and prescribed rotations acted mirrored

| | |
|---|---|
| Severity | High |
| Status | Draft (fix not merged) |
| Issue | [#435](https://github.com/lambdaclass/stabileo/issues/435) |
| Fix | PR [#436](https://github.com/lambdaclass/stabileo/pull/436) (open; the nodal-couple hunk is shared with #274) |
| Affected versions | up to 0.2.0 and `main` until the fix; introduction not yet bisected |
| Re-check registry | to add with the fix |

## Affected features

A plane (2D) model solved by the 3D solver (`shouldEmbedFlat2DModelIn3D`: the model is stood in
the X–Z plane, e.g. a plane model in 3D mode) with nodal moments or prescribed support rotations.

## What was wrong

The plane rotation θ (counter-clockwise as drawn) is a rotation about −Y once the model stands in
X–Z, so moments and prescribed rotations must change sign. They did not, so they **acted as their
mirror image**:

| Model | 2D solve | Embedded 3D solve |
|---|---|---|
| Two-span beam, nodal moment 10 kN·m at mid support: uz at the loaded node | +1.437e-4 m | −1.437e-4 m |
| Portal with nodal moments and forces: sway ux | 0.92 mm | 2.52 mm (2.7×) |
| Fixed support with prescribed rotation 0.002 rad | θ = +0.002 | θ = −0.002 |

Forces combined with moments can add instead of cancel, so drifts and member forces can be far too
large or far too small.

## How to tell if your model is affected

The model is flat (all nodes in one plane), was solved in 3D mode, and has nodal moments or
prescribed rotations. Rotational springs are not affected.

## Fixed in

Not yet fixed (PR #436).

## Workaround

Solve plane models in 2D mode, or enter nodal moments and prescribed rotations with the opposite
sign when solving them in 3D, and check one reaction against a hand calculation.

## What to do with results you already have

Re-run plane models with nodal moments or imposed rotations that were solved in 3D mode.

## Credits and references

Element-derivations review (PR #330, finding AC-15), confirmed through the app path (model store →
`buildSolverInput3D` → WASM).
