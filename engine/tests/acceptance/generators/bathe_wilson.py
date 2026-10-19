#!/usr/bin/env python3
"""Generate the Bathe & Wilson ten-bay, nine-storey plane frame eigenproblem
as a stabileo acceptance case.

Source: K.J. Bathe, E.L. Wilson, "Large Eigenvalue Problems in Dynamic
Analysis", J. Eng. Mech. Div. ASCE 98(EM6):1471-1485, 1972. The data are
restated in CSI's SAP2000 verification Example 1-021 and ETABS's matching
example: bays 20 ft, storeys 10 ft, fixed bases, every member E = 432,000
ksf, A = 3 ft^2, I = 1 ft^4, mass 3 kip-s^2/ft per ft of member length, shear
deformation ignored. The reference values are Bathe & Wilson's first three
eigenvalues omega^2 = 0.589541, 5.52695, 16.5878 (rad/s)^2. These are the
independent values that CSI quotes; CSI's own program output is not used.

Mass model: the 1972 eigenvalues are those of the member mass lumped half to
each end node in both translations (no rotary inertia). The replay below
gives 0.5895413, 5.5269559, 16.587870 that way. With a consistent mass
matrix, the third eigenvalue is 16.79 (+1.2%), so the case lumps the mass
too, on mass-only truss stubs (see sac_ohtori.py). The printed values match
the replay truncated, not rounded, to 6 significant figures (5.526956 ->
5.52695, 16.58787 -> 16.5878). So the tolerance is one unit in the last
printed digit.

Everything is converted to SI (kN, m, t): eigenvalues do not depend on units.

    python3 bathe_wilson.py [--replay]
"""
import json
import math
import os
import sys

FT = 0.3048
KSF_TO_MPA = 47.88025898 / 1000.0          # kip/ft^2 -> MPa
KIP_S2_PER_FT_TO_KG = 4448.2216152605 / FT  # kip*s^2/ft -> kg
E_MPA = 432000.0 * KSF_TO_MPA
A_M2 = 3.0 * FT**2
I_M4 = 1.0 * FT**4
MBAR = 3.0 * KIP_S2_PER_FT_TO_KG / FT       # kg per m of member
NB, NS = 10, 9
BAY, STOREY = 20.0 * FT, 10.0 * FT
STUB_E_MPA = 1e-6
REFS = ["0.589541", "5.52695", "16.5878"]


def frame():
    nodes = {}
    for r in range(NS + 1):
        for c in range(NB + 1):
            nodes[r * (NB + 1) + c + 1] = (c * BAY, r * STOREY)
    idx = lambda r, c: r * (NB + 1) + c + 1
    elems = [(idx(r, c), idx(r + 1, c)) for r in range(NS) for c in range(NB + 1)]
    elems += [(idx(r, c), idx(r, c + 1)) for r in range(1, NS + 1) for c in range(NB)]
    mass = {n: 0.0 for n in nodes}
    for a, b in elems:
        le = math.dist(nodes[a], nodes[b])
        mass[a] += MBAR * le / 2
        mass[b] += MBAR * le / 2
    return nodes, elems, mass


def case():
    nodes, elems, mass = frame()
    j_nodes = {str(n): {"id": n, "x": round(x, 9), "z": round(z, 9)} for n, (x, z) in nodes.items()}
    j_mat = {"1": {"id": 1, "e": E_MPA, "nu": 0.3}}
    j_sec = {"1": {"id": 1, "a": A_M2, "iz": I_M4}, "2": {"id": 2, "a": 1.0, "iz": 1e-12}}
    j_el = {}
    for i, (a, b) in enumerate(elems, start=1):
        j_el[str(i)] = {"id": i, "type": "frame", "nodeI": a, "nodeJ": b, "materialId": 1, "sectionId": 1,
                        "hingeStart": False, "hingeEnd": False}
    j_sup = {}
    sid = 0
    for n in range(1, NB + 2):
        sid += 1
        j_sup[str(sid)] = {"id": sid, "nodeId": n, "type": "fixed"}
    densities = {"1": 0.0}
    mats = {}
    nn, ne = 10000, 10000
    for n in sorted(nodes):
        if n <= NB + 1:
            continue
        m = mass[n]
        key = f"{m:.10e}"
        if key not in mats:
            mid = 100 + len(mats)
            mats[key] = mid
            j_mat[str(mid)] = {"id": mid, "e": STUB_E_MPA, "nu": 0.3}
            densities[str(mid)] = 3.0 * m
        x, z = nodes[n]
        j_nodes[str(nn)] = {"id": nn, "x": round(x + math.sqrt(0.5), 9), "z": round(z + math.sqrt(0.5), 9)}
        j_el[str(ne)] = {"id": ne, "type": "truss", "nodeI": n, "nodeJ": nn, "materialId": mats[key],
                         "sectionId": 2, "hingeStart": False, "hingeEnd": False}
        sid += 1
        j_sup[str(sid)] = {"id": sid, "nodeId": nn, "type": "fixed"}
        nn += 1
        ne += 1
    refs = []
    for i, printed in enumerate(REFS):
        ulp = 10 ** -len(printed.split(".")[1])
        # truncated print: the true value lies in [printed, printed + ulp);
        # centre the band on it so a symmetric check covers exactly that
        refs.append({"quantity": "eigenvalue", "mode": i + 1, "value": float(printed) + ulp / 2,
                     "tolAbs": ulp / 2, "sourceValue": float(printed), "sourceUnit": "(rad/s)^2",
                     "tolBasis": f"Bathe & Wilson print {printed}, truncated to 6 significant figures "
                                 f"(see generator): true value in [{printed}, {printed} + {ulp:g})"})
    return {
        "id": "bathe_wilson_10bay_9storey",
        "title": "Bathe & Wilson 10-bay, 9-storey plane frame eigenproblem",
        "dimension": "2d",
        "analysis": "modal",
        "numModes": 5,
        "densities": densities,
        "source": {
            "citation": "Bathe, K.J., Wilson, E.L. (1972). Large Eigenvalue Problems in Dynamic Analysis. "
                        "J. Eng. Mech. Div. ASCE 98(EM6):1471-1485.",
            "where": "Eigenvalues 0.589541, 5.52695, 16.5878 as quoted (independent reference column) in CSI "
                     "SAP2000 Software Verification, Example 1-021 'Bathe and Wilson eigenvalue problem'; same "
                     "data in the ETABS verification manual.",
            "independence": "Bathe & Wilson's own subspace-iteration results (1972); not a commercial program's "
                            "output, not computed by stabileo or by this generator.",
        },
        "modelling": [
            "Euler-Bernoulli frames, one element per member, fixed bases; kip-ft data converted to SI.",
            "Member mass lumped half to each end node in both translations (the 1972 mass model), carried by "
            "mass-only truss stubs; members have density 0. A consistent mass gives omega3^2 = 16.79 (+1.2%).",
        ],
        "references": refs,
        "model": {"nodes": j_nodes, "materials": j_mat, "sections": j_sec, "elements": j_el,
                  "supports": j_sup, "loads": [], "constraints": []},
    }


def replay():
    import numpy as np
    import scipy.linalg
    nodes, elems, mass = frame()
    n = len(nodes)
    E = E_MPA * 1e3  # kN/m^2
    K = np.zeros((3 * n, 3 * n))
    M = np.zeros(3 * n)
    for a, b in elems:
        (xa, za), (xb, zb) = nodes[a], nodes[b]
        L = math.hypot(xb - xa, zb - za)
        c, s = (xb - xa) / L, (zb - za) / L
        k = np.zeros((6, 6))
        k[np.ix_([0, 3], [0, 3])] = E * A_M2 / L * np.array([[1, -1], [-1, 1]])
        k[np.ix_([1, 2, 4, 5], [1, 2, 4, 5])] = E * I_M4 / L**3 * np.array(
            [[12, 6 * L, -12, 6 * L], [6 * L, 4 * L * L, -6 * L, 2 * L * L],
             [-12, -6 * L, 12, -6 * L], [6 * L, 2 * L * L, -6 * L, 4 * L * L]])
        T = np.eye(6)
        T[0:2, 0:2] = T[3:5, 3:5] = [[c, s], [-s, c]]
        d = [3 * (a - 1) + i for i in range(3)] + [3 * (b - 1) + i for i in range(3)]
        K[np.ix_(d, d)] += T.T @ k @ T
    for nd, m in mass.items():
        M[3 * (nd - 1)] = M[3 * (nd - 1) + 1] = m / 1000.0
    free = list(range(3 * (NB + 1), 3 * n))
    K, M = K[np.ix_(free, free)], np.diag(M[free])
    md = np.diag(M) > 0
    Kc = K[np.ix_(md, md)] - K[np.ix_(md, ~md)] @ np.linalg.solve(K[np.ix_(~md, ~md)], K[np.ix_(~md, md)])
    return scipy.linalg.eigh(Kc, M[np.ix_(md, md)], eigvals_only=True)[:3]


def main():
    c = case()
    path = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "models", c["id"] + ".json")
    with open(path, "w") as f:
        json.dump(c, f, indent=1)
        f.write("\n")
    print("wrote", os.path.relpath(path))
    if "--replay" in sys.argv:
        for i, (r, v) in enumerate(zip(REFS, replay())):
            print(f"  mode {i+1}: Bathe & Wilson {r:<9} replay {v:.7f}")


if __name__ == "__main__":
    main()
