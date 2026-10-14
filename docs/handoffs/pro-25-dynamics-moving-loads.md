# PRO — 25: dynamic and moving loads, units across PRO, names and table tools

Branch `feat/pro-25-dynamics-moving-loads`, on `feat/pro-24-cases-combinations`. Everything is in
the application layer; the engine is untouched.

## Spectra

- `engine/spectral-case.ts`: `UserSpectrum` (Sa in g or m/s², or Sd in m; linear or log
  interpolation), `userSa`, `userSpectrumPointsInG` (the engine's spectral analysis takes a table;
  log and Sd tables are sampled densely), `parseSpectrumTable` (refuses negative values and a period
  twice), `modeCoefficients` (k = Sd · Γ_d per mode, one direction), `modalCombine` (SRSS, CQC with
  `cqcRho`, ABS; the dominant mode's sign), `combineModes`, `directionCombine` / `combineDirections`,
  `spectralCaseResult`.
- One rule for a user table, shared by the case and the engine's table: outside the table Sa is held
  at its end values (an Sd table too: holding Sd made Sa grow as 1/T² below the first period, 32 g
  at 0.05 s). A linear Sa table keeps its T = 0 row for the engine (it was dropped, and the engine
  held the next row flat below it: the default spectrum read 1.0 g at 0.2 s against the case's 0.64 g).
- Directions (`SpectralCaseDef.directions`, percentages unless SRSS is chosen): each direction with
  a factor is worked out from the modes on its own (its modes combined, the dominant mode's sign),
  then the directions value by value: E = Σ f_d·|E_d| (100/30: |E₁| + 0.3·|E₂|) or √Σ (f_d·E_d)²,
  with the main direction's sign; (1, +0.3) and (1, −0.3) are one case. Per mode Sd · Σ f_d Γ_d was
  one oblique excitation: a corner column's N read 3.09 where the rule gives 4.10.
- A spectral case (`LoadCase.spectral`): each mode shape imposed at every node and solved once
  (`store/spectral-cases.ts` `imposedInput`, `spectralCaseResults`), on the modes' own structure
  (`dynamicInput`: the centerline, member offsets not expanded, linear: a tension-only brace is a
  brace). Imposed on the static model, offsets were expanded under modes found without them (a
  column's N 14.8 against the engine's 2.83) and a tension-only brace went slack under a mode.
  Reactions are the real supports' (`supportReactions`): the components they restrain, a spring's
  −k·u, nothing at free nodes or on a pinned support's rotations. Member diagrams are the straight
  line between the combined ends; a shell's Von Mises is a size.
- Modes are asked for until 90 % of the mass moves in each excited direction, Z included
  (`modalUntilMass(…, directions)`); short of it (modes run out, or 200), the solve says how much
  moved (`spectralCase.participationShort`; the engine's ratio is over the whole mass, so the mass
  at the supports never counts). A Z case on ten storeys read 1e-10 of its mass moving before.
- Combinations (`combination-methods.ts` `finishBundle`, `superposed`): a linear combination taking
  a spectral case is the rest of the combination as the solve gave it (linear, or second order under
  P-Delta per combination) plus the spectral part, factor by factor, with shells (components added,
  Von Mises from the sums, `combineShellStresses`) and pieces. The spectral part stays first order
  (`combos.spectral.firstOrder` says so on the combination); a combination with no second-order
  equilibrium gets no result. A nonlinear model refuses these combinations by name
  (`spectralRefusal`, `svc.spectralNonlinear`). They were rebuilt from the linear cases by the
  engine's combiner: no P-Delta, a buckled "40D+E" with results, no shell stresses.
- Not summed into "All loads" (`case-effects.ts` `singleSolveFactors`), not checked by the statics
  check and named there (`statics-rows.ts` `spectral`).
- A case on the code's spectrum keeps the γr/R written when it was ticked; when the regulation gives
  another, the case details and the solve say so (`staleCodeScale`), with a button to take it.
- Projects with constraints are refused (the engine withholds participation factors there).
- User spectra are kept in `model.dynamics.spectra` (`setDynamics` keeps the other fields);
  `ProUserSpectra.svelte` edits them; the advanced spectral analysis can run on one.
- `store/dynamic-input.ts`: one builder of the centerline input and the mass source for the advanced
  panel and the spectral cases.

## Vehicles

- `engine/vehicles.ts`: `Vehicle` (variable gap, gauge, dynamic factor), `AASHTO_VEHICLES`,
  `vehicleTrains`, JSON file, `trainModelLoads` (vertical global point loads, half on each path with
  two wheel lines), `positionsAlong`.
  - `vehicleProblem`: the variable gap names an axle by an integer index from 1 to the last and
    ranges over 0 < min ≤ max ≤ min + 100 m; gauge and dynamic factor finite and positive (a factor
    below 1 is allowed: it multiplies every axle). `vehicleFromJson` refuses a file that fails it
    (a gap from 0 to 100 km made 200 001 trains; a gap before axle 0.5 threw).
  - Steps: `usableStep`, at least `MIN_STEP` (0.05 m) in the logic, not only in the inputs (a
    spacing step of 0 never ended); `vehicleTrains` runs at most `MAX_SPACINGS` (100) spacings.
  - `positionPasses` / `passCount`: forward and, for an asymmetric train, back on the mirrored
    positions (`isSymmetric`, `reversedTrain`, moved here from the sweep, which uses them).
  - `alignSecondLine`: the second path is turned when its start lies nearer the first path's end
    (picked right to left, an axle's halves fell at 2 m and 14 m of a 16 m deck), and notes say
    when it is over 5° off, not as long (5 %), or not the gauge apart (±max(0.25 m, 10 %)). The
    gauge is that check; the loads follow the members picked.
  - The panel keeps weightless axle rows (dropping them moved the variable gap onto another axle);
    `trainModelLoads` writes nothing for them.
- `moving-loads-3d.ts`: `sweepTrains3D` (every spacing), `path2` (second wheel line).
- `store/moving-cases.ts` `addPositionCases(vehicle, trains, …, gauge)`: one Tr case per train,
  pass and position, all in one alternatives group (`move:<vehicle>`), so a combination takes one
  train at one position. It took the first spacing's forward pass only (HS20 over 2 × 10 m: pier
  −244.8 kN·m for the sweep's −289.2; HL-93 over 8 + 16 m: 661.8 for 681.3). The count is checked
  against `MAX_POSITION_CASES` before anything is built. Notes: the second line's mismatches, and
  another vehicle's group multiplying the combinations (two groups of 150: 22,500 per rule).

## Generator

- Wind (`load-plan-wind.ts`): `wind.region` (node ids: the levels and the front are theirs; roof
  loads only on members inside) and `wind.profile` (net lateral pressure against height, mean over
  each band exactly by `profileMean`; case 1, one internal-pressure variant, no roof pressures, no
  §2.1.5 minimum). `ProActionRegion.svelte`, `ProfileChart.svelte`.
  - An empty region (`[]`: a box or group with no node, a zone without an outline) is no wind and
    `loadPlan.note.windEmptyRegion`; it was read as the whole model.
  - The band under a region's lowest level reaches halfway to the model's level below it, not the
    ground (a box over the upper storeys gave +6 m 90 kN for its 60).
  - Wa goes through the same region (roof loads on its members only) and profile (no roof loads,
    one internal-pressure variant). On a profile, Wa is the profile times (V_service / V)², the
    ratio of the velocity pressures (G not re-read); it was the profile itself, so Wa = W.
  - A profile with a structure other than a building is not used: `loadPlan.note.windProfileOther`,
    also shown under the profile in the dialog.
- Snow (`snow-loads.ts`): `snow.roof` (member ids) in place of the detected roof, in the projection
  and the panel paths and the corner loads.
  - The chosen members are read as the roof is (`roofMembers(model, only)`): `gravityLayout` over
    the chosen members alone, so beam-like members (|Δz|/L ≤ 0.5) that nothing else chosen covers.
    A region brought the columns (a 90° slope: 52° for a 14° roof, C_s 0.45 for 1, zero line loads
    on the columns) and the floors under the roof. Nothing outside the choice covers it, so a lower
    roof can be chosen alone. By panels, a chosen member takes its roof panels, or all its panels
    when the whole model reads it as covered (`onRoof`).
  - `roof: []` is no roof: `snow.note.emptyRegion` (it loaded the detected roof).
- `model/loads/region-nodes.ts`: a region (all, zone, group, box) as nodes and members.
  `ProActionRegion` says when a region holds no node.

## Mass weights

- `MassSource.weights` (`mass-source.ts`, kept by `normalizeMassSource` and through a change of the
  factors): members kN/m, slabs kN/m², floor kN/m² to the beams (`floor-definitions.ts`), on a
  region. `engine/dynamics/mass-weights.ts` writes them as loads of a case of their own that the
  mass source takes at factor 1. `ProMassWeights.svelte` in the mass-source panel.
- Slabs: shells sloping 30° at most (Newell normal, |n_z| ≥ cos 30°, the members' `beamLike`
  rule), so a sloped roof is a slab and a wall is not (a 5 × 4 m slab and a 5 × 3 m wall at
  2 kN/m² gave 7.14 t for 4.08).
- A floor on the whole model (the panel's default region) is every level the horizontal members
  make, one `level` target each; it was dropped without a word.

## Pushover

- `engine/pushover-pattern.ts`: lateral patterns along X or Y by each node's dead-load weight
  (uniform, triangular by height, the dominant mode's shape), adding up to 1 kN; `upToTarget` (base
  shear or control-node displacement, the last step interpolated). The advanced panel adds the
  pattern, the case and the target; `PushoverView` reads the curve up to the target, and says when
  the run ended below it (`pro.po.targetUnreachedShear` / `…Disp`: the curve shown is the whole run).
- What a pushover needs, and where it stands: present are the hinge sequence, the capacity curve and a
  control node; added here are the load pattern, the target and a choice of case. Missing, and the
  engine's: gravity held constant while the lateral pattern grows, FEMA-type hinge curves with
  IO/LS/CP acceptance, geometric nonlinearity in the pushover, and a performance point against a
  demand spectrum.

## Adding supports and loads

- One way to add each: the panel's "Add support" / "Add load" card (`WriteInPanelButton verb="add"`).
  The Draw buttons for supports and loads are gone, and so are their options in `ProDrawBar` and
  the PRO click branches in `Viewport3D` (with `drawState.nodalLoad` / `memberLoad`). A support or
  load tool armed in Basic is put down when PRO's draw bar mounts.
- The support card ends in the loads' `LoadTargetPicker` (`entity="nodes"`): selection, numbers,
  group or range, resolved by `resolveTargets`, in one batch; a node with a support takes the new one.
- The model's context menu on a node, in PRO, selects it and opens the card on the selection
  (`drawState.writeOnSelection`, `writeSeq`); the load card turns to a node kind.
- `LoadTargetPicker`: the group filter's precedence fixed (a floor-load definition was offered).

## Self-weight in the Add load card

- `ProWriteLoadCard`: a fourth group of kinds, General, with Self-weight (direction, factor). It
  writes `analysis.selfWeight` (the rule the solve already reads): `all` is the whole model, a group
  is `groupId`, any other target an `elements` list. A rule with the same case, direction and reach
  is replaced. `ProSelfWeight.svelte` is gone; the rules are a table at the top of
  `ProLoadTables` (case, direction and factor edited in place; removed there). PRO does not offer
  the older D-case switch.
- A PRO project whose first member or shell is drawn in PRO is given `selfWeight: []`
  (`selfWeightRuleEffect`), so a new project has no self-weight until it is added. An empty model is
  not given it while empty: the effect notes the `loadEpoch` it saw it empty at, and only structure
  appearing in that same model, with the panel still mounted (`leaveSelfWeightRule` on the panel's
  `onDestroy`), gets `[]`. Writing `[]` into any empty model PRO saw made a column drawn next in
  Basic weigh 1.6 kN there and 0 back in PRO, and an example opened in PRO (`loadExample` clears,
  then awaits the fixture) lost its rule when the effect ran in that gap (pro-edificio-7p ΣRz
  15579 → 5770 kN).
- One with structure and no rule (an older file, a Basic model, a link, a tab) is migrated as
  before, and its rule shows in the table. Structure is members, quads or plates
  (`hasStructure`, and `loadExample`'s own check): a project of slabs alone was given `[]` and
  lost its weight (a 4×4 m slab ΣRz 252 → 1 kN).
- `load-targets.ts`: `{ by: 'all' }`. `LoadTargetPicker` offers it with `allowAll`. For the
  self-weight, on the whole model or a group the count beside Add is members and slabs
  (`selfWeightReach`, as `selfWeightScope` weighs a group), and a group of slabs alone is offered
  (`groupOffered`).
- A table row whose case or direction is changed into another row's (same reach) merges into it
  with the edited factor, and a toast says so (`editSelfWeightRule`); two such rows weighed twice.
- Add support on many nodes is `modelStore.addSupports` (one pass, one map copy, one undo step,
  one support per node); per node it was quadratic (8000 nodes: seconds).
- The picker no longer shows the count: it hands `summary` to the card, which shows it beside the
  Add button (`WriteCard`'s `aside` snippet for supports). On the selection, when the pointer is not
  selecting what it needs, a button beside the choice switches it (`load-target-activate`).

## Units, names and table tools

From issues #251, #181, #96, #205 and #206.

### Units

- `utils/units.ts`:
  - A fourth display system, `SImm`: SI with lengths in mm (coordinates, displacements, section properties). Forces, loads per metre and per square metre and stresses stay as in SI; files stay in SI.
  - `temperatureDiff`: a ΔT or a gradient, no offset. The thermal labels in the 3D view used `temperature`, which added 32 °F to a difference.
  - `sectionArea`, `sectionInertia`, `sectionModulus`, `sectionDim`: shown in cm in SI, as section tables give them.
  - `planArea`: a building's area (a tributary area, the wind's openings and walls, a cladding element), m² in SI, SI (mm) and MKS, ft² in imperial. `area` is a cross-section's (cm² in MKS, mm² in SI (mm), in²): `ProCaseDetails` A_T, `ProWindCasesPanel` A₀/A_g/A₀i/A_gi and `ProWindCladding` showed 50 m² as 500 000 cm².
  - Stirrup and tie spacings (`RebarEditorBeam`, `RebarEditorColumn`, `BatchEditDialog`) are `sectionDim` (cm, mm in SI (mm), in), and the column's s,max beside them follows it; they were `length` (ft in imperial) beside an s,max in cm.
  - `isUnitSystem` guards the stored preference.
- `QuantityInput`:
  - nullable (an empty field is the default its placeholder names);
  - `above`: a strict lower bound (a gauge, a spring: 0 is not one), so a 0 the parent drops is not left on show (`moving-gauge`, `sup-k-*`);
  - placeholder, `describedBy`, `showUnit` and `onchange` (once, on leaving: one undo step for a table cell);
  - the bound value follows the typing, and the field is not rewritten while it has the focus.
- The Add load card (`ProWriteLoadCard`, `ProShellLoadForm`): every magnitude is a `QuantityInput`.
  - Imposed displacements and tendon eccentricities used to be typed in mm. They are now `displacement` and `length` in the display system (mm in SImm).
  - ε₀ stays in ‰.
- Inputs moved to the unit layer (the sweep found 177 tags in 50 files reading SI without it):
  - **Modelling:**
    - `ProNodesTab`: table cells convert at the edge; the write card uses `QuantityInput`.
    - `ProQuickEdit`, `PlacementHud`, `ProDrawBar`.
    - `ProGridPanel`: bays and storey lists in display units.
    - `ProTransformPanel`, `ProEditPanel` (fill thickness and element size), `ProMesher`.
    - `ProSurfaces` (length keys only), `ProShellTab`, `spec/SpecSurfaces`.
    - `ProConstraintsTab` (offsets, springK, springKr), `ProMemberBehaviour`, `SupportDofFields`.
    - `ProSupportEditor` (springs), `ProSteelLbEditor`, `ProStairSection`.
    - `ProViewPanel` (note position), `property/MemberOffsetEditor`.
  - **Loads:**
    - `ProLoadTables`: every cell, with units in the headers.
    - `LoadTargetPicker` range, `ProActionRegion`, `ProCaseDetails`, `ProFloorLoadSection` box, `ProDeadLoadBuilder`.
    - `ProSnowSection`, `ProSpecialLoadsSection`, `ProWindCasesPanel`, `ProWindCladding`, `ProWindDynamics`, `ProWindStructure`.
    - `ProAutoLoadsApplying`, `ProAutoLoadsDialog` (altitude), `ProMovingLoadsPanel`.
    - `dynamics/ProMassWeights`, `dynamics/TimeHistoryPanel`.
    - `ProResultsTab` (query threshold and its readouts), `ProFoundationSprings`.
  - **Analysis, design, materials:**
    - `ProAdvancedTab`: pushover target, Winkler, SSI.
    - The section builder in `sectionDim`, and the section readout (A, I, J, W) in `sectionArea`/`sectionInertia`/`sectionModulus`.
    - `ProMaterialsTab`: E, γ, fy in display units with the unit in the header.
    - `material/CustomMaterialPanel`.
    - `design/FoundationsPanel`, `design/BatchEditDialog`, `design/DetailingWorkflow`, `design/RebarEditorBeam`, `design/RebarEditorColumn`.
    - `generators/*`: the "(m)" in the steel dictionaries' generator labels and hints is gone.
    - `ProConnectionsTab` (its SI fields only).
- Left as they are, with a fixed unit on purpose:
  - mm: connections, rebar, drawn sections, cold-formed, weld tolerance, taper.
  - cm²: shear areas.
  - Also %, ‰, degrees, s, Hz, ratios and counts.
  - The support force–displacement curve, typed as "mm kN".
  - Shell contour limits (kN·m/m has no Quantity).
- Read-only texts still with a fixed SI unit:
  - `loadTarget.chainOk` (m), the floor-load level names.
  - The wind qz profile table, the cladding pressure header.
  - The time-history summary, the δmax line of the advanced tab.
  - The loose P-Δ of Basic (`ToolbarAdvanced`) publishes without the stability check; Basic takes it in its own PR.

### Names

- `Node.name`, `Element.name`, optional:
  - kept in the `.ded`; a file without them opens as before;
  - `renameNode` / `renameElement` record on the views undo channel (`restoreViewsOnly` restores the names), so renaming keeps the results;
  - `updateNode` keeps the name on a move.
- Edited in the nodes and members tables (a Name column after the id). A new row welded onto an existing node gives it its name only when it has none (`actions/node-rows.ts` `addNodeRow`): it renamed "Apoyo A" to the row's "B2".
- Labels: `MEMBER_LABELS` has `name`. Members show their name; nodes too when labels read names (`nodeLabelText`, one line in `results-sync.ts`, nothing else of the labels touched).
- Excel: `name` column in Nodes and Members, written and read (`schema.ts`, `parse.ts`, `load-fixture.ts`). A name starting with "-" is written with "−" (`safeText`, no formula) and read back with "-" (`unguardText`).
- Report: the nodes and members tables add the Name column when some entity has one.

### Load sketches

- `loads/LoadSketch.svelte`: an SVG beside the Add load card's fields, one drawing per kind (node forces and moments with the global axes, inclined force, imposed displacement, distributed trapezoid/triangle/hydrostatic with a and b, point load at a, thermal profile across the depth, strain, prestress tendon through eI/eM/eJ, self-weight, slab area loads by field, fluid against a wall, point on a slab, slab temperature).
- Drawn from the card's state (`SketchInput`; the slab form hands its own through `bind:sketch`): signs set the arrows, magnitudes scale against each other, empty fields show their symbol. Schematic, not to the model's scale.
- Members are drawn sloping so local, global and projected axes differ; stations along the member (a, b, peak, a of a point load) are dimensioned from I; the prestress eccentricities are dimensioned from the axis. Thermal loads are the temperature diagram across the section (member: ΔTgz = T(−z) − T(+z), ΔTgy across the width; plate: ΔTg = T(+z) − T(−z), the engine's sign for shells). Plate area loads: side on (direction, true or projected area, the projection drawn) and in plan (uniform, corners, along an axis, the partial rectangle).
- The corner button shows the sketch large over the viewport's canvas, or the whole screen on a phone; the overlay is moved to `document.body` (`utils/portal.ts`) so the panel's stacking context does not keep it under the header. Escape closes it.
- The card: case and kind on top (the kind a `<select>` with an optgroup per Node / Member / Plate / General, testid `wl-kind`); the fields on one grid (`.fg-r`: a right-aligned name column, cells of one width, the unit after the row; vectors under X, Y, Z or I, J headers), set in under a rule; Apply to and Add at the end. The sketch stays to the right, shrinks to 120 px, then wraps under the fields; on a phone it goes under them.
- An inclined force: an origin (a node or a point) toward the loaded node, each node Apply to names; the target is not typed. An origin on the loaded node is refused (no direction).

### Tools

- **Loose P-Δ** (`ProAdvancedTab.handlePDelta`): `amplification()` and `converged`, the same criterion as per combination. Unstable or not converged: not published, an earlier P-Δ result is cleared, and the panel says why (`pdelta-refused`). A run that fails (the engine's error, a throw) clears the earlier result, its refusal and its report line too (`forgetPDelta`).
- **Report:**
  - `exportReportAs` is async;
  - `screenshotWithNumbers` turns on node and member numbers (ids, not on the selection only), waits three frames, captures, and puts back what the reader had;
  - the picture is Figure 1, and the figures section continues from 2.
- **Tables:** a double click on a node or member row selects it and dispatches `stabileo-zoom-to-selection`, not on a cell being edited. The members table keeps the place of the "Open specifications" button, which used to appear on the first click and move the row under the pointer.
- **Clean-up** (`ProEditPanel`):
  - exact duplicates are counted from `checkModel` (`MODEL_DUPLICATE_ELEMENT`) and removed as before;
  - overlapping collinear members come from `overlappingCollinearWarnings`, are listed by pair with their shared length and are selected to decide.
- **CAD tests:**
  - `cad-real-dxf` and `cad-rooms-uploaded` warn when the client plans are absent and say so in the suite title.
  - `cad-real-like.test.ts` builds a plan with author layer names, block columns and a wrong `$INSUNITS`, and runs the shipped path.

## Generator form

- `generators/GenSection.svelte`: a `<details>` per group of the form (shed: Frames, Columns, Roof, Bracing; truss: Shape, Web members; column; structures: Geometry; then Sections and material, Generated structures, Where), testid `gen-sec-<id>`. They start folded and fold again on picking a generator (`gen-help.svelte.ts`, `foldAllSections`); folded, the heading shows a summary (`gen-sec-sum-<id>`). The fields stay in the DOM, so `aria-describedby` and the U1 test still find them; the e2e helper `pickGenerator` opens them all (`openGenSections`).
- `generators/GenRow.svelte`: one parameter on the load card's grid (name right-aligned, box in the next column, unit after; a checkbox in the box column). The explanation is kept beside the field, visually hidden, under `gen-hint-<key>` (structure fields: `gen-hint-f-<key>`, text `generator.fhint.*`), and shown in the help line at the top of the dock for the row under the pointer or in focus.
- The help line is one line tall, beside the preview's pin; a longer explanation opens as a card over the drawing (absolute, `pointer-events: none`), never pushing anything. At first it grew with the text, which shrank the scroller: at the bottom of the list the hovered row moved out from under the pointer and the form shook. `m2-generators-ui.spec.ts` checks the row does not move. On a touch screen the idle invitation is hidden (`hover: none`).
- The Place / Preview / Regenerate actions are a footer of their own (`.gen-actions`), outside the dock: always at the foot of the panel, pinned preview or not. On a phone (≤ 767 px) the preview starts unpinned, so the short sheet keeps room for the form; picking a generator scrolls the form to its top.
- `TrussFields` takes `part: 'shape' | 'web'`; the shed's two previews sit side by side when the panel is wide enough, so the dock keeps the Place button in sight.

## Selection panel

- `components/SelectionPanel.svelte` is shared with Basic; the behaviour is the same in both, plates only in PRO.
- Every operation goes through `applyArmed`: armed kinds take the operation's answer (or keep their selection when it gives none), the others are emptied. Changing the kind drops the selection of kinds no longer armed, the isolation `basic-selection-permutations.spec.ts` already states for drags. In PRO the store does it (`uiStore` `dropUnarmed`): Multi turned off, a kind button, the `selectMode` setter (`PickKind`, table rows); not a tab that switches the kind (`proActiveTab`), so the context menu's card opens on the node it selected (`select-kinds-narrow.test.ts`).
- Every operation reads the model as the view shows it (`{ ...model, ...visibleModel() }`), as All and Invert do: like, parallel, loaded in case, the walk with nothing selected (`allOfKind`) and by id (`selectShownByIds`, which names the hidden ids, `selection.hiddenIds`).
- Same level is one band for every kind, `LEVEL_TOL` = `GROUP_TOL.z` (0.15 m), the storey band members were already grouped by (nodes, supports and plates used 1 mm). Same plane compares the signed offset along the seed's normal (walls at x = ±5 are not coplanar; two plates either side of vertical, their normals flipped apart by `shellPlane`, are).
- `lib/model/select-like.ts`: "like the selection" per kind (`LIKE_OPS`), plates parallel to a global axis or plane, and `focusNodeIds` (what zoom to selection frames: member ends, plate corners, supported nodes, loaded nodes/members/plates). `Viewport3D`'s zoom-to-selection handler reads it; that one line is the only change there.
- Loaded in case: each armed kind takes its part (disabled with only supports armed). Parallel to: members and plates (disabled otherwise). Walk: through the armed kind. Previous: `selectionHistory.back(kinds)` gives back the last selection of the armed kinds and skips the others.
- By id: nodes, members, plates (one kind: `selectByIds(..., 'shells')` looks in triangles and quadrilaterals and reports an id found in both), supports, loads. The list follows the armed kind, and naming another kind arms it.

## Merged with main and with #264

- `main` went into PRO 24 first (signed merge), then PRO 24 into PRO 25, then `basic/interaction-view` (#264) into PRO 25, so this PR carries #264 until it is merged: merge #264 first.
- Where they met:
  - `solver-service.ts`: main's plan (`combinationsPlan3D`, refusals, imposed displacements) with PRO 24's case effects and cases to solve, and PRO 25's spectral cases (`finishBundle(…, spectral)`), in both the sequential and the workers' entry.
  - `write-load.ts` and `shell-load-form.ts` (main's pure builders of the Add load card) take the card's SI values (`null` for a blank field); the inclined force goes from an origin (a node or a point) toward each loaded node (`writeLoad.fromNodeMissing`, `writeLoad.fromSelf`). Their tests were moved to numbers.
  - `centerlineInput(opts)` (`store/dynamic-input.ts`) takes main's `caseDisplacements` and `uncut`.
  - One temperature-difference quantity, `temperatureDelta` (#264's name; PRO 25 had `temperatureDiff`), and one set of section quantities.
  - `SelectionPanel`: Basic as #264 left it; PRO by armed kind. One walk for both (`selectWalkItem` takes plates); Basic's "like the selection" uses `likeMembers` from `select-like.ts`.
- The WASM engine must be rebuilt after the merge (`npm run wasm`): main changed the Rust side.

## Nodes, scrollbars, selection and temperature in PRO

- Nodes are drawn a fixed size on screen in PRO too (`NodesInstanced.setStyle`, `uiStore.nodeStyle3D`, default `auto`); the mesh stays as the click target. `resolveNodeStyle(pref, tool, aiming)`: in PRO, aiming is also `shellNodePick.active`, `placementStore.active` and the connections tab. Settings › node style and panel scrollbars are shown in PRO.
- Scrollbars: `styles/panel-scrollbars.css`, class `st-panel-scrollbars`, on Basic's panel and PRO's (`data-testid="pro-panel"`), never on the phone.
- Zoom to selection frames one node with a third of the model around it, in both modes.
- Temperature: the signs along the member (#264's `drawThermalSigns`) show in PRO; PRO keeps its names (ΔT, ΔTgz, ΔTgy; ΔTg on plates), because it has two gradients and "ΔTg" is already the plate gradient.
- A load on a plate is picked on its fill (`LoadArrowsBatched.markArea`, `pickLoadsWithDistance(…, areas)`): the fill is the part the load covers (`shellLoadExtent`: the region with its openings out, the range of a `vary`), and two fills under the pointer go nearer first (`projectToScreen` gives the depth), then smaller, then by id. The load tables move to the case of a load selected in the model (and so does the Add load card, which writes to the case the tables show); a model from Basic keeps its self-weight case (`migrateSelfWeightIfNeeded`).

## Large models

- Measured with a real GPU (Metal) on a generated building (`e2e/large-model.ts`: 1,638 nodes, 4,245 members, 1,200 slab faces, 5,130 loads): members 860 → ~40 ms to open, loads 942 → ~80 ms, plates 203 → ~30 ms, nodes 144 → ~40 ms. On the industrial shed example: members ~25 ms, loads ~38 ms, no long frame.
- `lib/utils/progressive-rows.svelte.ts` (`progressiveRows`, `progressiveLimit`): first 30 rows, then a batch a frame; `reach(i)` draws up to a row the model asks for. Used by Basic's members table, PRO's members (40 a frame), nodes, plates (60) and load tables (one budget of 30 a frame over the eight tables).
- `components/tables/LazySelect.svelte`: a select holding its list only while open; `LazyIdSelect` is built on it. Used for material and section per member row, material per plate row, frame per distributed and point load row.
- The rest of the drawing time is the browser's style and layout of the large tables (a CPU profile shows it native), spread over frames. Virtualizing would be the next step if a table must stay smaller.
- `e2e/pro-large-model-tables.spec.ts` counts rows and options on the first frame (not times: CI draws WebGL on the CPU).

## Tests

- `engine/__tests__/spectral-case.test.ts`: the engine's spectral displacements for the same modes,
  spectrum and rule (CQC, SRSS); an imposed mode reproduced; the rules and the sign; user spectra;
  a user spectrum with the code's values.
- `engine/__tests__/spectral-case-review.test.ts` (the review's findings, each failing before):
  100/30 and SRSS of directions; combinations under P-Delta, past buckling, on a nonlinear model,
  with shells; reactions at the real supports and springs; a Z case's modes and the short-mass
  warning; Sd tables and T = 0 for the engine; table validation; tension-only braces and member
  offsets against the engine's spectral analysis (outside PRO: inside, the engine's spectral member
  forces differ from K·u for the same input, which is the engine's to look at); the stale code
  scale; "All loads"; the statics check; the pushover target.
- `engine/__tests__/vehicles.test.ts`: HS20-44, spacings, dynamic factor, file; position cases
  against the sweep; two wheel lines.
- `engine/loads/__tests__/pro25-actions.test.ts`: wind profile and region, mass weights, pushover
  pattern and target.
- After review:
  - `engine/__tests__/vehicles-positions.test.ts`: position cases over every spacing and both ways
    equal to the sweep (HS20 pier moment, HL-93 reversed pass), the cap before building, the
    multiplying groups, vehicle validation and steps, weightless rows, the second line oriented and
    checked.
  - `engine/loads/__tests__/pro25-regions.test.ts`: snow on a roof chosen by a box (by width and by
    panels, half a roof, a flat tower), empty regions for snow and wind, Wa on a profile and on a
    region, a profile on another structure, the band under a box's lowest level.
  - `engine/dynamics/__tests__/mass-weights.test.ts`: a floor on the whole model, slabs without walls.
- `e2e/pro-dynamics-loads.spec.ts` (`@smoke`).
- `e2e/pro-units-names-tools.spec.ts` (`@smoke`): the card in MKS, SImm and Imperial; names in the tables and the label mode; double click; duplicates and overlaps; P-Δ refused; the report figure.
- `lib/utils/__tests__/units-mks.test.ts` (SImm, section quantities, temperatureDiff), `lib/store/__tests__/entity-names.test.ts`, `lib/cad/__tests__/cad-real-like.test.ts`.
- `e2e/pro-add-supports-loads.spec.ts` (`@smoke`): every option of both cards, on the selection and
  on numbers, and the context menu.
- `lib/model/__tests__/select-like.test.ts`: like by kind, plates parallel, by id over every kind, zoom framing.
- `e2e/pro-selection-tools.spec.ts` (`@smoke` block): supports, by id, previous by kind, loaded in case with nodes.
- `e2e/m2-generators-ui.spec.ts` (`@smoke` block): folded sections with summaries; the hovered row does not move.

## Not done here

- The constant-gravity pushover, FEMA hinges and the performance point (the engine's pending list).
- Nodal and concentrated mass weights need nodal masses in the engine.
- Spectral cases are not computed by the live (on-edit) solve.
- The tributary area of each member is not drawn in 3D.

## Exports

**Contains #271** (`basic/exports`), merged in for its export dialogs (`components/export/ExportDialog`, `ExportSection`) and its paper figures (`lib/export/view-figure.ts`, `figure-3d.ts`, `svg-context.ts`, `paper-ink.ts`): merge #271 before this PR.

Project › Export has three groups, and every button opens a dialog that says what comes out, everything on by default and what is turned off remembered in the browser:

- **Data and results: Excel and CSV** (`components/pro/export/ProDataExportDialog.svelte`). One dialog for both, the format a choice inside:
  - sheets by name, model and results, with all/none (`MODEL_SHEET_NAMES` in `export/workbook-model.ts`, `RESULT_SHEET_NAMES` in `export/workbook-results.ts`). A sheet left out is not built (`want` in `modelSheets` and `resultSheets`), so the stations of a large model cost nothing when not asked for;
  - cases and combinations (`SourcePick` in `store/project-workbook.ts`); the envelope and the maxima are taken over what is chosen;
  - points along the members, offered only when a sheet that reads them is chosen. This is what the panel's bare "Estaciones 5 / 13 / Críticas" was;
  - CSV: always a zip, one file per sheet with the cover and the conventions (`downloadProjectCsv`), as `proExport.dataAlways` says; one sheet used to come alone. The format note says the separator is a comma and the decimal a point, whatever the language.
  - It solves first when results are asked for and the model has none (`pro/export-results.ts`, `ensureProResults`, PRO's own `runGlobalSolve`). If the solve fails the file has the model sheets and says so.
- **Report: PDF report** (`ProReportDialog.svelte`, rewritten on the shared frame). Only the PDF: the report's Excel button and `workbookOptions` are gone, since Export › Excel chooses its own sheets.
  - The project, content and figures sections fold, each with a one-line summary.
  - Each model table can be left out (`sections.parts`).
  - Each result (reactions, displacements, member forces, shell results) goes as a table, a figure or both (`resultModes`).
  - The figures are drawn from the model (`reportResultFigures` in `pro/report-export.ts`), as SVG in data URLs:
    - the numbered model is Figure 1, in place of the screen capture, which stays as the fallback when no figure can be drawn;
    - the loaded model goes under the loads;
    - each result's figures go under its table.
  - `pro-report.ts` numbers figures in the order they appear (`figureHtml`), and the figures added from the view keep going at the end.
- **Drawings and pictures: DXF, SVG, PNG, JPG.**
  - **DXF** (`ProDxfExportDialog.svelte`, `pro/dxf-export.ts`, `dxf/writer.ts`, rewritten). The old writer drew 2D results only, so in PRO it gave the bare geometry.
    - In space (`space`) or projected: plan, front XZ or side YZ. In m, cm or mm (`$INSUNITS`).
    - A layer per content: members, nodes, supports, shells as `3DFACE`, numbering, loads with one layer per case, deformed shape (members and shells), M, V and N in the plane the 3D view draws them, with a layer per component, and reactions. The RESULTS layer states the set and the scales.
    - Loads: temperatures and tendons as text on their member or shell, imposed displacements as an arrow and their components (mm, rad); a kind still not drawn is named on the LOADS layer, and a case with nothing drawn declares no layer. Two case names that read alike as a layer name ("Viento +X", "Viento X") each take their case id.
    - An SRSS or ABS set (`AnalysisResults3D.magnitude`) is named as one (`magnitudeLabel` in `result-scopes.ts`: "SRSS, magnitudes without a sign") in `resultSetChoices`, in `figureTitle` (the pictures and the report's figures) and on the RESULTS layer, and its moments are printed unsigned (the screen's sign flip is for signed sets).
    - R12, with 3D polylines (flag 8) so every vertex keeps its z.
  - **Pictures** (`ProViewExportDialog.svelte`). Basic's figures plus, from `figure-3d.ts`:
    - shells as faces in every figure, the nearest last, and moved in the deformed one;
    - `shellMap`: a shell result per element in the screen's contour colours (a grey ramp in black and white) with its scale;
    - fixed views (`FigureOptions.view`: plan, front, side, isometric);
    - results of any case, combination or the envelope (`FigureOptions.results`, `resultsName`), read from the store without switching the screen;
    - JPG (`ViewFormat`). Contour colours bypass the paper ink through `RAW_INK` in `paper-ink.ts`.
- Removed as dead once PRO stopped calling them: `exportDXF`/`downloadDXF`, `exportSVG`/`downloadSVG`, `generateReportHTML`/`openPDFReport` in `store/file.ts`, and `DxfExportOptions`/`DXF_COLORS`.

Tests: `lib/export/__tests__/pro-exports.test.ts` covers:
- the workbook narrowed by sheet and by combination;
- the DXF layers, faces, units, projection and per-case loads;
- the shell map's colours and its black-and-white ramp;
- the heading of another set;
- the report's figures, the parts left out and the figure numbering;
- a one-sheet CSV with its cover and conventions; an SRSS set named in the choices, the titles and the DXF, its moments unsigned; thermal, tendon and imposed-displacement loads drawn, no empty layer, a layer per case for names that read alike.

Review fixes have their own tests beside the code: `select-like.test.ts` (hidden items, signed plane, one level band, a triangle's load framed), `select-kinds-narrow.test.ts`, `units-mks.test.ts` (`planArea`), `load-pick.test.ts` (depth and size of fills, `shellLoadExtent`), `entity-names.test.ts` (a row welded onto a named node, a "-" name through Excel).

`e2e/pro-exports.spec.ts` covers DXF, SVG, JPG and the report on `mat-foundation`. `e2e/pro-project-workbook.spec.ts` covers the Excel dialog with 13 points and a one-sheet CSV.

## Stairs out of PRO

The Stair section of the shells tab is gone (`ProStairSection.svelte`, the `stair` pick target, `e2e/pro-stairs.spec.ts`). Stairs belong to the design desk's stairs editor, which already shares the flight fields and links the analysis plates. `lib/model/stair.ts`, `stair-convert.ts`, `components/pro/stairs/StairFlightFields.svelte` and their tests stay for it.
