#!/usr/bin/env python3
"""Generate the SAC LA 3-, 9- and 20-storey benchmark frames (Ohtori et al. 2004)
as stabileo acceptance cases.

Source (public): Y. Ohtori, R.E. Christenson, B.F. Spencer Jr., S.J. Dyke,
"Benchmark Control Problems for Seismically Excited Nonlinear Buildings",
J. Eng. Mech. 130(4):366-385, 2004. The problem statement and its MATLAB
model files (struct3.m, struct9.m, struct20.m, Bld_NLBM.m) are distributed by
the NEES "Database for Structural Control and Monitoring Benchmark Problems",
doi:10.4231/D3Q52FC7T (https://datacenterhub.org/resources/257), archive
entries Report_bldg_full3.pdf / Files_bldg_full3.zip.

Every number below is transcribed from struct#.m (units N, m, kg); the
modelling rules (lumped masses m/2 per beam end in both translations, rigid
floor by master/slave horizontal DOFs, element types, supports) are those of
Bld_NLBM.m. The reference frequencies are the ones printed in the paper
(p. 12 of the report): they are NOT computed here.

    python3 sac_ohtori.py            # (re)write ../models/sac_*.json
    python3 sac_ohtori.py --replay   # also replay Bld_NLBM.m's eigen-analysis
                                     # with numpy (needs numpy) and print it

stabileo has no lumped nodal-mass input (masses come from element density
only), so each nodal mass is carried by a mass-only truss stub: a 1 m truss
of area 1 m^2 from the node to a fixed auxiliary node, density 3*m kg/m^3.
Its consistent truss mass puts exactly 2/6*rho*A*L = m on the free node in
both translations (the fixed end's share is restrained), and its axial
stiffness E*A/L = 1e-3 kN/m (E = 1e-6 MPa) is ~1e-8 of a storey stiffness.
The benchmark's rotational masses (alpha = 1e-6, "rotational inertia is
ignored") are dropped; --replay shows the effect on the reported digits is nil.
"""
import json
import math
import os
import sys

E_STEEL_MPA = 200000.0  # any E works: A and I are back-computed from EA and EI
STUB_E_MPA = 1e-6
STUB_DIR = (math.sqrt(0.5), math.sqrt(0.5))  # 45 deg, away from beams/columns


def splice(mat, a, b):
    """Bld splice rule: (6/13)*mat(a) + (7/13)*mat(b) for (EI1, EA)."""
    return tuple((6 / 13) * mat[a][k] + (7 / 13) * mat[b][k] for k in range(2))


# ---------------------------------------------------------------- 3 storey
# mat id: (EI1 [N m^2], EA [N])
MAT3 = {
    1: (2.8295e08, 9.7518e09), 2: (3.6034e08, 1.1790e10), 3: (4.9100e08, 1e10),
    4: (4.1028e08, 1e10), 5: (1.5229e08, 1e10), 6: (1.0070e07, 2.5798e09),
    7: (7.0155e07, 1e10),
}
SAC3 = dict(
    name="sac_la_3story",
    title="SAC LA 3-storey pre-Northridge MRF (N-S frame), Ohtori et al. 2004",
    heights=[3.9624] * 3,
    widths=[9.1440] * 4,
    mats=MAT3,
    col_mats=[[1, 2, 2, 1, 6]] * 3,
    beam_mats=[[3, 3, 3, 7], [4, 4, 4, 7], [5, 5, 5, 7]],
    # (1-based beam index, Bld element type): 3 = pinned-pinned (axial only)
    beam_types={4: 3, 8: 3, 12: 3},
    masters=[(6, [7, 8, 9, 10]), (11, [12, 13, 14, 15]), (16, [17, 18, 19, 20])],
    # node: (fix_h, fix_v, fix_rot)
    fixed={n: (1, 1, 1) for n in range(1, 6)},
    floor_mass=[4.7835e05, 4.7835e05, 5.1779e05],
    reference_hz=["0.99", "3.06", "5.83"],
)

# ---------------------------------------------------------------- 9 storey
MAT9 = {
    1: (4.5272e08, 1.4060e10), 2: (6.8324e08, 1.8962e10), 3: (5.9836e08, 1.7285e10),
    4: (3.1957e08, 1.0745e10), 5: (2.8295e08, 9.7518e09), 6: (2.5049e08, 8.8360e09),
    7: (8.1140e08, 6.0629e09), 8: (6.4912e08, 5.1212e09), 9: (3.3205e08, 3.7539e09),
    10: (2.3718e08, 3.1992e09), 11: (1.5229e08, 2.5929e09),
}
MAT9[12] = splice(MAT9, 2, 3)
MAT9[13] = splice(MAT9, 1, 4)
MAT9[14] = splice(MAT9, 3, 1)
MAT9[15] = splice(MAT9, 4, 5)
MAT9[16] = splice(MAT9, 5, 6)
m9 = [4.8159e05, 5.0349e05, 4.9473e05, 5.3414e05]
SAC9 = dict(
    name="sac_la_9story",
    title="SAC LA 9-storey pre-Northridge MRF (N-S frame), Ohtori et al. 2004",
    heights=[3.6576, 5.4864] + [3.9624] * 8,
    widths=[9.1440] * 5,
    mats=MAT9,
    col_mats=[
        [1, 2, 2, 2, 2, 1], [1, 2, 2, 2, 2, 1], [1, 12, 12, 12, 12, 1],
        [1, 3, 3, 3, 3, 1], [13, 14, 14, 14, 14, 13], [4, 1, 1, 1, 1, 4],
        [15, 13, 13, 13, 13, 15], [5, 4, 4, 4, 4, 5], [16, 15, 15, 15, 15, 16],
        [6, 5, 5, 5, 5, 6],
    ],
    beam_mats=[[7] * 5] * 3 + [[8] * 5] * 4 + [[9] * 5, [10] * 5, [11] * 5],
    # type 5 = fixed at node a, pinned at node b (right-hand bay of each level)
    beam_types={k: 5 for k in (5, 10, 15, 20, 25, 30, 35, 40, 45, 50)},
    masters=[(m, [m - 2, m - 1, m + 1, m + 2, m + 3]) for m in range(15, 64, 6)],
    fixed={**{n: (1, 1, 0) for n in range(1, 7)}, **{n: (1, 0, 0) for n in range(7, 13)}},
    floor_mass=[m9[0], m9[1]] + [m9[2]] * 7 + [m9[3]],
    reference_hz=["0.443", "1.18", "2.05", "3.09", "4.27"],
)

# --------------------------------------------------------------- 20 storey
MAT20 = {
    1: (2.4959e08, 1.3416e10), 2: (9.9037e08, 1.2693e10), 3: (1.8176e08, 8.8686e09),
    4: (1.5305e08, 7.2239e09), 5: (6.3667e08, 8.6687e09), 6: (5.2099e08, 7.2626e09),
    7: (1.2076e08, 5.5147e09), 8: (3.3456e08, 4.9664e09), 9: (2.9462e08, 4.4375e09),
    10: (8.4639e07, 3.7410e09), 11: (1.9724e08, 3.1863e09), 12: (3.3207e08, 3.7539e09),
    13: (3.7201e08, 4.0892e09), 14: (2.3719e08, 3.1992e09), 15: (1.2900e08, 2.3478e09),
    16: (8.1893e07, 1.8963e09),
}
for mid, (a, b) in {17: (1, 3), 18: (3, 4), 19: (2, 5), 20: (5, 6), 21: (4, 7),
                    22: (6, 8), 23: (8, 9), 24: (7, 10), 25: (9, 11)}.items():
    MAT20[mid] = splice(MAT20, a, b)
m20 = [2.6561e5, 2.8166e5, 2.7582e5, 2.9188e5]
_c20 = (
    [[1, 2, 2, 2, 2, 1]] * 3 + [[17, 2, 2, 2, 2, 17]] + [[3, 2, 2, 2, 2, 3]] * 2
    + [[18, 19, 19, 19, 19, 18]] + [[4, 5, 5, 5, 5, 4]] * 5 + [[4, 20, 20, 20, 20, 4]]
    + [[4, 6, 6, 6, 6, 4]] * 2 + [[21, 22, 22, 22, 22, 21]] + [[7, 8, 8, 8, 8, 7]] * 2
    + [[7, 23, 23, 23, 23, 7]] + [[7, 9, 9, 9, 9, 7]] + [[24, 25, 25, 25, 25, 24]]
    + [[10, 11, 11, 11, 11, 10]]
)
_b20 = ([12] * 6 + [13] * 6 + [12] * 3 + [8] * 3 + [14] * 2 + [15, 16])
SAC20 = dict(
    name="sac_la_20story",
    title="SAC LA 20-storey pre-Northridge MRF (N-S frame), Ohtori et al. 2004",
    heights=[3.6576, 3.6576, 5.4864] + [3.9624] * 19,
    widths=[6.0960] * 5,
    mats=MAT20,
    col_mats=_c20,
    beam_mats=[[m] * 5 for m in _b20],
    beam_types={k: 3 for k in (1, 2, 3, 4, 5)},
    masters=[(9, [7, 8, 10, 11, 12])] + [(m, [m - 2, m - 1, m + 1, m + 2, m + 3]) for m in range(21, 136, 6)],
    fixed={**{n: (1, 1, 0) for n in range(1, 7)}, **{n: (1, 0, 0) for n in range(13, 19)}},
    floor_mass=[m20[0], m20[0], m20[1]] + [m20[2]] * 18 + [m20[3]],
    reference_hz=["0.261", "0.753", "1.30", "1.83", "2.40", "2.44", "2.92", "3.01", "3.63", "3.68"],
)


def frame(b):
    """Nodes, elements and lumped masses exactly as Bld_NLBM.m builds them."""
    nb, ns = len(b["widths"]), len(b["heights"])
    assert len(b["col_mats"]) == ns and len(b["beam_mats"]) == ns
    assert len(b["floor_mass"]) == ns
    xs = [0.0]
    for w in b["widths"]:
        xs.append(xs[-1] + w)
    zs = [0.0]
    for h in b["heights"]:
        zs.append(zs[-1] + h)
    nodes = {}
    for r in range(ns + 1):
        for c in range(nb + 1):
            nodes[r * (nb + 1) + c + 1] = (xs[c], zs[r])
    elems = []  # (na, nb, mat, type, mass)
    for s in range(ns):
        for c in range(nb + 1):
            na = s * (nb + 1) + c + 1
            elems.append((na, na + nb + 1, b["col_mats"][s][c], 1, 0.0))
    k = 0
    for s in range(ns):
        for j in range(nb):
            k += 1
            na = (s + 1) * (nb + 1) + j + 1
            elems.append((na, na + 1, b["beam_mats"][s][j], b["beam_types"].get(k, 2),
                          b["floor_mass"][s] / nb))
    mass = {n: 0.0 for n in nodes}
    for (na, nb_, _m, _t, em) in elems:
        mass[na] += em / 2
        mass[nb_] += em / 2
    return nodes, elems, mass


def stabileo_case(b):
    nodes, elems, mass = frame(b)
    E = E_STEEL_MPA * 1e6  # Pa
    j_nodes, j_elems, j_sup, j_mat, j_sec = {}, {}, {}, {}, {}
    j_mat["1"] = {"id": 1, "e": E_STEEL_MPA, "nu": 0.3}
    for mid, (ei, ea) in sorted(b["mats"].items()):
        j_sec[str(mid)] = {"id": mid, "a": ea / E, "iz": ei / E}
    for n, (x, z) in nodes.items():
        j_nodes[str(n)] = {"id": n, "x": round(x, 6), "z": round(z, 6)}
    eid = 0
    for (na, nb_, mid, typ, _em) in elems:
        eid += 1
        e = {"id": eid, "type": "frame", "nodeI": na, "nodeJ": nb_, "materialId": 1,
             "sectionId": mid, "hingeStart": False, "hingeEnd": False}
        if typ == 3:
            e["type"] = "truss"
        elif typ == 5:
            e["hingeEnd"] = True
        elif typ not in (1, 2):
            raise ValueError(typ)
        j_elems[str(eid)] = e
    sid = 0
    for n, (h, v, r) in sorted(b["fixed"].items()):
        sid += 1
        kind = {(1, 1, 1): "fixed", (1, 1, 0): "pinned", (1, 0, 0): "rollerZ"}[(h, v, r)]
        j_sup[str(sid)] = {"id": sid, "nodeId": n, "type": kind}
    # mass-only truss stubs (see module docstring)
    densities = {"1": 0.0}
    stub_sec = max(b["mats"]) + 1
    j_sec[str(stub_sec)] = {"id": stub_sec, "a": 1.0, "iz": 1e-12}
    mat_for_mass = {}
    next_node = max(nodes) + 1
    fixed_dofs = b["fixed"]
    for n in sorted(nodes):
        m = mass[n]
        if m <= 0.0:
            continue
        fx = fixed_dofs.get(n, (0, 0, 0))
        if fx[0] and fx[1]:
            continue  # both translations restrained: the mass never moves
        key = f"{m:.10e}"
        if key not in mat_for_mass:
            mid = 100 + len(mat_for_mass)
            mat_for_mass[key] = mid
            j_mat[str(mid)] = {"id": mid, "e": STUB_E_MPA, "nu": 0.3}
            densities[str(mid)] = 3.0 * m
        x, z = nodes[n]
        aux = next_node
        next_node += 1
        j_nodes[str(aux)] = {"id": aux, "x": round(x + STUB_DIR[0], 6), "z": round(z + STUB_DIR[1], 6)}
        eid += 1
        j_elems[str(eid)] = {"id": eid, "type": "truss", "nodeI": n, "nodeJ": aux,
                             "materialId": mat_for_mass[key], "sectionId": stub_sec,
                             "hingeStart": False, "hingeEnd": False}
        sid += 1
        j_sup[str(sid)] = {"id": sid, "nodeId": aux, "type": "fixed"}
    constraints = [
        {"type": "equalDOF", "masterNode": m, "slaveNode": s, "dofs": [0]}
        for (m, slaves) in b["masters"] for s in slaves
    ]
    model = {"nodes": j_nodes, "materials": j_mat, "sections": j_sec, "elements": j_elems,
             "supports": j_sup, "loads": [], "constraints": constraints}
    refs = []
    for i, printed in enumerate(b["reference_hz"]):
        # half a unit in the last digit the paper prints (kept as text so
        # "1.30" keeps its trailing zero)
        f = float(printed)
        decimals = len(printed.split(".")[1])
        refs.append({"quantity": "frequency", "mode": i + 1, "value": f,
                     "tolAbs": 0.5 * 10 ** (-decimals),
                     "tolBasis": f"paper prints {printed} Hz; half a unit in the last printed digit"})
    total = sum(mass.values())
    return {
        "id": b["name"],
        "title": b["title"],
        "dimension": "2d",
        "analysis": "modal",
        "numModes": len(b["reference_hz"]) + 2,
        "densities": densities,
        "source": {
            "citation": "Ohtori, Christenson, Spencer, Dyke (2004). Benchmark Control Problems for "
                        "Seismically Excited Nonlinear Buildings. J. Eng. Mech. 130(4):366-385.",
            "where": "Report_bldg_full3.pdf p.12 (natural frequencies of the evaluation model); "
                     "model data struct" + b["name"].split("_")[-1].replace("story", "") + ".m and Bld_NLBM.m in "
                     "Files_bldg_full3.zip, NEES database doi:10.4231/D3Q52FC7T",
            "independence": "Published by the benchmark authors from their own MATLAB model; "
                            "not computed by stabileo or by this generator.",
        },
        "modelling": [
            "Euler-Bernoulli plane frame, nodes at beam-column joints, no panel zones (as the source).",
            "Sections back-computed from the source's EI1 and EA with E = 200000 MPa.",
            "Rigid floor: equalDOF on ux between each level's master and slave nodes (source's slv_tbl).",
            "Lumped masses (m/2 per beam end, both translations) carried by mass-only truss stubs; "
            "the source's ~1e-6 rotational masses are omitted.",
            f"Total lumped mass {total:.6e} kg (source: half the building's seismic mass).",
        ],
        "references": refs,
        "model": model,
    }


def replay(b, rot_alpha=1e-6):
    """Bld_NLBM.m's eigen-analysis, with numpy (for the docs, not the test)."""
    import numpy as np
    nodes, elems, _ = frame(b)
    n = len(nodes)
    K = np.zeros((3 * n, 3 * n))
    M = np.zeros((3 * n, 3 * n))
    for (na, nb_, mid, typ, em) in elems:
        EI, EA = b["mats"][mid]
        (xa, za), (xb, zb) = nodes[na], nodes[nb_]
        L = math.hypot(xb - xa, zb - za)
        c, s = (xb - xa) / L, (zb - za) / L
        T = np.eye(6)
        T[0:2, 0:2] = T[3:5, 3:5] = [[c, s], [-s, c]]
        k = np.zeros((6, 6))
        k[np.ix_([0, 3], [0, 3])] = EA / L * np.array([[1, -1], [-1, 1]])
        if typ in (1, 2):
            i = EI / L
            k[np.ix_([1, 2, 4, 5], [1, 2, 4, 5])] = i * np.array([
                [12 / L**2, 6 / L, -12 / L**2, 6 / L], [6 / L, 4, -6 / L, 2],
                [-12 / L**2, -6 / L, 12 / L**2, -6 / L], [6 / L, 2, -6 / L, 4]])
        elif typ == 5:
            k[np.ix_([1, 2, 4], [1, 2, 4])] = 3 * EI / L * np.array([
                [1 / L**2, 1 / L, -1 / L**2], [1 / L, 1, -1 / L], [-1 / L**2, -1 / L, 1 / L**2]])
        dofs = [3 * (na - 1) + d for d in range(3)] + [3 * (nb_ - 1) + d for d in range(3)]
        K[np.ix_(dofs, dofs)] += T.T @ k @ T
        mi = em / 2 * np.diag([1, 1, rot_alpha * L**2 / 210] * 2)
        M[np.ix_(dofs, dofs)] += mi
    nbp = len(b["widths"]) + 1
    for nd, (h, v, r) in b["fixed"].items():
        if nd <= nbp and r == 0:  # Bld: rotational mass on pinned bases
            M[3 * nd - 1, 3 * nd - 1] = M[3 * (nd + nbp) - 1, 3 * (nd + nbp) - 1]
    fixed = set()
    for nd, fx in b["fixed"].items():
        for d in range(3):
            if fx[d]:
                fixed.add(3 * (nd - 1) + d)
    T = np.eye(3 * n)
    for m, slaves in b["masters"]:
        for s in slaves:
            T[3 * (s - 1), 3 * (s - 1)] = 0
            T[3 * (s - 1), 3 * (m - 1)] = 1
    free = [i for i in range(3 * n) if i not in fixed]
    Tr = T[np.ix_(free, free)]
    Tr = Tr[:, [j for j in range(len(free)) if Tr[j, j] != 0]]
    Kr = Tr.T @ K[np.ix_(free, free)] @ Tr
    Mr = Tr.T @ M[np.ix_(free, free)] @ Tr
    if rot_alpha == 0.0:  # condense massless DOFs statically
        md = np.diag(Mr) > 0
        Kmm, Kms, Kss = Kr[np.ix_(md, md)], Kr[np.ix_(md, ~md)], Kr[np.ix_(~md, ~md)]
        Kr = Kmm - Kms @ np.linalg.solve(Kss, Kms.T)
        Mr = Mr[np.ix_(md, md)]
    import scipy.linalg
    w2 = scipy.linalg.eigh(Kr, Mr, eigvals_only=True)
    return np.sqrt(np.abs(w2)) / (2 * math.pi)


def main():
    here = os.path.dirname(os.path.abspath(__file__))
    out = os.path.join(here, "..", "models")
    for b in (SAC3, SAC9, SAC20):
        case = stabileo_case(b)
        path = os.path.join(out, b["name"] + ".json")
        with open(path, "w") as f:
            json.dump(case, f, indent=1, sort_keys=False)
            f.write("\n")
        print("wrote", os.path.relpath(path))
        if "--replay" in sys.argv:
            nref = len(b["reference_hz"])
            f1 = replay(b)[:nref]
            f0 = replay(b, 0.0)[:nref]
            for i, ref in enumerate(b["reference_hz"]):
                print(f"  mode {i+1}: paper {ref:<6} Bld replay {f1[i]:.6f}  "
                      f"(no rotational mass {f0[i]:.6f})")


if __name__ == "__main__":
    main()
