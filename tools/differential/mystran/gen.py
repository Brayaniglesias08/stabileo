"""MYSTRAN-specific generators.

modal3d_lumped: the shared modal3d models with the torsion of every member
released at one end, where another, non-parallel member provides the
node's stiffness about that axis (so that no mechanism is created; members
with no such end are left as they are and the model is then unsupported). MYSTRAN's BAR mass is lumped (rho A L / 2 per end,
no rotary inertia); stabileo's frame mass is lumped the same way only for
members with a released end (engine/src/solver/mass_matrix.rs), so these
models have the same mass matrix in both programs. Models that become
mechanisms are skipped by the driver's static probe.
"""

import os
import sys

sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "common"))
import gen  # noqa: E402  (common/gen.py)


def modal3d_lumped(seed):
    import math
    m = gen.modal3d(seed)
    xyz = {n["id"]: (n["x"], n["y"], n["z"]) for n in m["nodes"].values()}
    els = list(m["elements"].values())
    for e in els:
        e["type"] = "frame"

    def axis(e):
        a, b = xyz[e["nodeI"]], xyz[e["nodeJ"]]
        d = [b[k] - a[k] for k in range(3)]
        n = math.sqrt(sum(x * x for x in d))
        return [x / n for x in d]

    def braced(e, node):
        u = axis(e)
        return any(abs(sum(p * q for p, q in zip(u, axis(o)))) < 0.99
                   for o in els if o is not e and node in (o["nodeI"], o["nodeJ"]))

    for e in els:
        if braced(e, e["nodeJ"]):
            e["releaseTEnd"] = True
        elif braced(e, e["nodeI"]):
            e["releaseTStart"] = True
    return m


GENERATORS = {"modal3d_lumped": modal3d_lumped}
ANALYSIS = {"modal3d_lumped": "modal"}
