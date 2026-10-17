# Differential testing against independent frame programs

The result certificates (`engine/src/certify`) prove that stabileo's
displacements solve the system it assembled. They cannot see a wrong element
matrix, fixed-end force, axis or sign convention, a dropped reaction term, or
a singular stiffness solved without a warning: a wrong model is certified
too. These harnesses solve the same model in independent programs and compare
displacements, reactions, member end forces, frequencies and mode shapes.

| program | harness | analyses | install |
|---|---|---|---|
| [Frame3DD](frame3dd/README.md) | `frame3dd/` | static 2D/3D, modal, P-Delta | `frame3dd/build.sh` (source, patched for double precision) |
| [PyNite](pynite/README.md) | `pynite/` | static 2D/3D (releases, trusses, springs) | `pip install PyNiteFEA` |
| [Mystran](mystran/README.md) | `mystran/` | see its README | see its README |
| [Kratos](kratos/README.md) | `kratos/` | see its README | `pip install KratosMultiphysics KratosStructuralMechanicsApplication` |
| [CalculiX](calculix/README.md) | `calculix/` | see its README | see its README |

OpenSees has its own harness in `tools/opensees/` (whose model conventions
and comparison `common/` reuses) and Code_Aster one in `tools/differential/code_aster`
when present.

## Run

```sh
cargo build --release --example diff_solve          # the stabileo side (engine/examples/diff_solve.rs)
python3 -m venv tools/differential/.venv
tools/differential/.venv/bin/pip install numpy scipy PyNiteFEA KratosMultiphysics \
    KratosStructuralMechanicsApplication KratosLinearSolversApplication openseespy
tools/differential/frame3dd/build.sh

cd tools/differential
.venv/bin/python run.py frame3dd --gen frame3d 300 --gen modal3d 100 --gen pdelta3d 100 -j 8
.venv/bin/python run.py pynite --gen rich3d_eb 1000 --gen frame2d 300 -j 8 --dump-dir out/pynite --shrink
.venv/bin/python run.py pynite ../../engine/tests/fixtures/*-input.json
```

`ci.sh` runs every available harness with the CI sizes (the nightly
`differential` workflow).

Inputs are model files (stabileo solver JSON, e.g. `engine/tests/fixtures/*-input.json`)
and generators (`--gen NAME N`, seeds `--seed-start..+N`, deterministic):
`rich3d` (tools/opensees/gen_models.py: trusses, releases, local-y vectors,
roll angles, Timoshenko sections, springs, settlements, partial/trapezoidal
and point loads), `frame2d`, `frame3d`, `building`, `modal3d`, `pdelta3d`
(`common/gen.py`) and per-program ones (`<program>/gen.py`). `--dump-dir`
writes every flagged model; `--shrink` also writes a delta-debugged minimal
model (`.min.json`) and, when the adapter can, its native input file.

Exit status: 0 nothing flagged, 1 a model flagged, 2 harness error.

## Statuses

| status | meaning |
|---|---|
| `ok` | agrees within tolerance |
| `mismatch` | disagrees: a stabileo bug, a translator bug or a reference bug, to triage |
| `known` | a mismatch fixed on an open PR not merged into this branch (`common/known.py`) |
| `silent_singular` | the reference's stiffness is singular (Jacobi-scaled κ > 1e14) and stabileo solved it without a warning |
| `false_reject` | stabileo refused a model whose reference stiffness is regular (κ < 1e10) |
| `false_warning` | stabileo flagged a model ill-conditioned whose reference stiffness is well conditioned (κ < 1e8) |
| `ill_conditioned` | stabileo warned (singular/indefinite, LU fallback, high residual), or the reference κ > 1e12: no unique answer to compare |
| `stabileo_failed` | stabileo refused the model (and no reference says it is regular) |
| `unstable` | P-Delta past the first buckling load |
| `unsupported` | the reference program cannot represent a feature of the model |
| `reference_failed`, `unloaded` | the reference program failed; the model has no load |

The last-but-one block is how the singularity checks are tested: a model is
never "compared" when one side says it has no unique answer, but the two
sides must agree on whether it does.

## Comparison

Static and P-Delta results use the OpenSees harness's scale-aware comparison
(`tools/opensees/compare.py`): each group (translations, rotations, reaction
forces / moments, end forces / moments) is compared as
`|stabileo − reference| / scale`, with the scale the group's largest
magnitude floored by its companion group times the model size. The default
tolerance is 1e-6, widened to `100 ε κ` when the reference reports κ (both
programs lose about κ ε). Modal results compare frequencies (relative) and
mode shapes as subspaces of clustered (repeated) frequencies
(`common/results.py`).

A tolerance is never widened to make a disagreement go away: each one is a
stabileo bug (fixed in the engine, with a regression test in
`engine/tests/differential_regressions.rs`), a translator bug (fixed here),
or a documented modelling difference of the reference program (made
`unsupported`, or worked around exactly, e.g. `common.model.nodalize`).

## Layout

```
run.py              command line
common/driver.py    one comparison per model, statuses, shrinking, summary
common/model.py     stabileo model I/O, geometry, 2D -> 3D lifting, nodalizing, isolation
common/results.py   static and modal comparison
common/gen.py       random models
common/known.py     mismatches fixed on open PRs
<program>/adapter.py  translator: stabileo JSON -> native input -> results
```

An adapter provides `NAME`, `ANALYSES`, `available()`,
`unsupported(model, analysis)`, `run(model, analysis, densities=, n_modes=)`
and optionally `TOL`, `write_native` (see `common/driver.py`).

## Findings

See each program's README. In summary (this branch):

- **Silent mechanisms** (PyNite, OpenSees): a member hinged at the tip of
  another member and free at its other end spins freely; rounding left its
  Cholesky pivot at ~1e-14 of the diagonal, above the old 64 ε singularity
  threshold, so stabileo returned arbitrary displacements without a warning.
  The threshold is now 1e-10 (`engine/src/solver/definiteness.rs`, with the
  argument why it cannot flag a model whose Jacobi-scaled condition number is
  below 1e10); regression `dangling_hinged_member_is_flagged_singular`.
- Frame3DD bugs found and worked around (README there): swapped axial share
  and crossed shear corrections of point-load fixed-end forces; modal
  elimination of restrained DOFs left their coupling to lower DOFs.
