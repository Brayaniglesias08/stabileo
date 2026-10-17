//! What the review reads from the model's geometry: member directions, floor levels and their
//! plan areas, whether the model looks like a building at all, and its connected parts.

use std::collections::HashMap;

use super::{SanityInput, SanityMaterial, SanitySection};

/// A member's orientation relative to the vertical.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Orientation {
    Vertical,
    Horizontal,
    Diagonal,
}

/// Within this angle of vertical or horizontal, a member counts as such (sin ≈ 0.035, 2°).
const ALIGN_SIN: f64 = 0.035;

#[derive(Debug, Clone)]
pub struct MemberGeom {
    pub id: usize,
    pub i: usize,
    pub j: usize,
    pub pi: [f64; 3],
    pub pj: [f64; 3],
    pub length: f64,
    /// Unit vector I → J.
    pub dir: [f64; 3],
    pub orientation: Orientation,
}

#[derive(Debug, Clone)]
pub struct Level {
    /// Elevation, along the up axis.
    pub z: f64,
    /// Plan area of the convex hull of the floor's nodes, m².
    pub area: f64,
}

pub struct ModelGeometry<'a> {
    pub up: usize,
    /// The two plan axes.
    pub plan: [usize; 2],
    pub nodes: HashMap<usize, [f64; 3]>,
    pub materials: HashMap<usize, &'a SanityMaterial>,
    pub sections: HashMap<usize, &'a SanitySection>,
    pub members: Vec<MemberGeom>,
    /// Largest extent of the bounding box, m.
    pub extent: f64,
    /// Elevation of the supports (or the lowest node), the base of a building.
    pub base: f64,
    /// Floor levels above the base, lowest first.
    pub levels: Vec<Level>,
    /// Tolerance used to group elevations into levels.
    pub level_tol: f64,
    /// The model reads as a multi-storey building (see `is_building`).
    pub building: bool,
    /// Why it does not, in English, when it does not.
    pub not_building_reason: String,
}

impl<'a> ModelGeometry<'a> {
    pub fn new(input: &'a SanityInput) -> Self {
        let up = input.up.index();
        let plan = if up == 2 { [0, 1] } else { [0, 2] };
        let nodes: HashMap<usize, [f64; 3]> = input.nodes.iter().map(|n| (n.id, n.p())).collect();
        let materials = input.materials.iter().map(|m| (m.id, m)).collect();
        let sections = input.sections.iter().map(|s| (s.id, s)).collect();

        let mut members = Vec::with_capacity(input.members.len());
        for m in &input.members {
            let (Some(&pi), Some(&pj)) = (nodes.get(&m.node_i), nodes.get(&m.node_j)) else { continue };
            let d = sub(pj, pi);
            let length = norm(d);
            if !(length > 1e-12) {
                continue;
            }
            let dir = scale(d, 1.0 / length);
            let s_up = dir[up].abs();
            let orientation = if s_up > 1.0 - ALIGN_SIN * ALIGN_SIN / 2.0 {
                Orientation::Vertical
            } else if s_up < ALIGN_SIN {
                Orientation::Horizontal
            } else {
                Orientation::Diagonal
            };
            members.push(MemberGeom { id: m.id, i: m.node_i, j: m.node_j, pi, pj, length, dir, orientation });
        }

        let mut lo = [f64::INFINITY; 3];
        let mut hi = [f64::NEG_INFINITY; 3];
        for p in nodes.values() {
            for k in 0..3 {
                lo[k] = lo[k].min(p[k]);
                hi[k] = hi[k].max(p[k]);
            }
        }
        let extent = if nodes.is_empty() { 0.0 } else { (0..3).map(|k| hi[k] - lo[k]).fold(0.0, f64::max) };

        let supported: Vec<f64> = input.supports.iter().filter_map(|s| nodes.get(&s.node_id)).map(|p| p[up]).collect();
        let base = if !supported.is_empty() {
            supported.iter().cloned().fold(f64::INFINITY, f64::min)
        } else if !nodes.is_empty() {
            lo[up]
        } else {
            0.0
        };
        let height = if nodes.is_empty() { 0.0 } else { hi[up] - base };
        let level_tol = (0.002 * height).max(0.05);

        // Floor nodes: the ends of horizontal members, and the corners of horizontal shells.
        let mut floor_pts: Vec<[f64; 3]> = members
            .iter()
            .filter(|m| m.orientation == Orientation::Horizontal)
            .flat_map(|m| [m.pi, m.pj])
            .collect();
        for s in &input.shells {
            let pts: Vec<[f64; 3]> = s.nodes.iter().filter_map(|n| nodes.get(n).copied()).collect();
            if pts.len() < 3 {
                continue;
            }
            let zs: Vec<f64> = pts.iter().map(|p| p[up]).collect();
            let span = zs.iter().cloned().fold(f64::NEG_INFINITY, f64::max) - zs.iter().cloned().fold(f64::INFINITY, f64::min);
            if span <= level_tol {
                floor_pts.extend(pts);
            }
        }
        let mut levels = Vec::new();
        if !input.planar {
            let mut zs: Vec<f64> = floor_pts.iter().map(|p| p[up]).filter(|z| *z > base + level_tol).collect();
            zs.sort_by(|a, b| a.partial_cmp(b).unwrap());
            let mut groups: Vec<Vec<f64>> = Vec::new();
            for z in zs {
                match groups.last_mut() {
                    Some(g) if z - *g.last().unwrap() <= level_tol => g.push(z),
                    _ => groups.push(vec![z]),
                }
            }
            for g in groups {
                let z = g.iter().sum::<f64>() / g.len() as f64;
                let plan_pts: Vec<[f64; 2]> = floor_pts
                    .iter()
                    .filter(|p| (p[up] - z).abs() <= level_tol)
                    .map(|p| [p[plan[0]], p[plan[1]]])
                    .collect();
                levels.push(Level { z, area: hull_area(&plan_pts) });
            }
        }

        let mut g = ModelGeometry {
            up,
            plan,
            nodes,
            materials,
            sections,
            members,
            extent,
            base,
            levels,
            level_tol,
            building: false,
            not_building_reason: String::new(),
        };
        let (b, why) = g.is_building(input.planar);
        g.building = b;
        g.not_building_reason = why;
        g
    }

    /// A multi-storey building, for the checks whose bands only hold for buildings: at least two
    /// floor levels above the base with a real plan area (≥ 25 m²) each, and at least four
    /// vertical columns. A plane model has no floor area; a lattice tower, a truss, a bridge or a
    /// single-storey shed is not what the bands were drawn from.
    fn is_building(&self, planar: bool) -> (bool, String) {
        if planar {
            return (false, "a plane model has no floor area".into());
        }
        let floors = self.levels.iter().filter(|l| l.area >= 25.0).count();
        if floors < 2 {
            return (false, format!("{} floor level(s) with a plan area of 25 m² or more above the base; a multi-storey building has at least 2", floors));
        }
        let columns = self.members.iter().filter(|m| m.orientation == Orientation::Vertical).count();
        if columns < 4 {
            return (false, format!("{} vertical column(s); a building frame has at least 4", columns));
        }
        (true, String::new())
    }

    pub fn floors(&self) -> impl Iterator<Item = &Level> {
        self.levels.iter().filter(|l| l.area >= 25.0)
    }

    /// Height of the top floor above the base.
    pub fn height(&self) -> f64 {
        self.levels.last().map_or(0.0, |l| l.z - self.base)
    }

    pub fn elevation(&self, p: [f64; 3]) -> f64 {
        p[self.up]
    }

    pub fn plan_of(&self, p: [f64; 3]) -> [f64; 2] {
        [p[self.plan[0]], p[self.plan[1]]]
    }
}

// ---------- small vector helpers ----------

pub fn sub(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
pub fn scale(a: [f64; 3], s: f64) -> [f64; 3] {
    [a[0] * s, a[1] * s, a[2] * s]
}
pub fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
pub fn cross(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}
pub fn norm(a: [f64; 3]) -> f64 {
    dot(a, a).sqrt()
}

/// Area of the convex hull of plan points (Andrew's monotone chain).
pub fn hull_area(pts: &[[f64; 2]]) -> f64 {
    let mut p: Vec<[f64; 2]> = pts.to_vec();
    p.sort_by(|a, b| a[0].partial_cmp(&b[0]).unwrap().then(a[1].partial_cmp(&b[1]).unwrap()));
    p.dedup_by(|a, b| (a[0] - b[0]).abs() < 1e-9 && (a[1] - b[1]).abs() < 1e-9);
    if p.len() < 3 {
        return 0.0;
    }
    let turn = |o: [f64; 2], a: [f64; 2], b: [f64; 2]| (a[0] - o[0]) * (b[1] - o[1]) - (a[1] - o[1]) * (b[0] - o[0]);
    let mut hull: Vec<[f64; 2]> = Vec::with_capacity(2 * p.len());
    for &q in &p {
        while hull.len() >= 2 && turn(hull[hull.len() - 2], hull[hull.len() - 1], q) <= 0.0 {
            hull.pop();
        }
        hull.push(q);
    }
    let lower = hull.len() + 1;
    for &q in p.iter().rev().skip(1) {
        while hull.len() >= lower && turn(hull[hull.len() - 2], hull[hull.len() - 1], q) <= 0.0 {
            hull.pop();
        }
        hull.push(q);
    }
    hull.pop();
    let mut a = 0.0;
    for k in 0..hull.len() {
        let (u, v) = (hull[k], hull[(k + 1) % hull.len()]);
        a += u[0] * v[1] - v[0] * u[1];
    }
    a.abs() / 2.0
}

/// Area of a planar polygon in 3D (triangle or quad), by the cross product.
pub fn polygon_area(pts: &[[f64; 3]]) -> f64 {
    if pts.len() < 3 {
        return 0.0;
    }
    let mut n = [0.0; 3];
    for k in 1..pts.len() - 1 {
        let c = cross(sub(pts[k], pts[0]), sub(pts[k + 1], pts[0]));
        n = [n[0] + c[0], n[1] + c[1], n[2] + c[2]];
    }
    norm(n) / 2.0
}

/// Union-find over node ids.
pub struct Components {
    parent: HashMap<usize, usize>,
}

impl Components {
    pub fn new() -> Self {
        Components { parent: HashMap::new() }
    }
    pub fn find(&mut self, x: usize) -> usize {
        let p = *self.parent.entry(x).or_insert(x);
        if p == x {
            return x;
        }
        let r = self.find(p);
        self.parent.insert(x, r);
        r
    }
    pub fn union(&mut self, a: usize, b: usize) {
        let (ra, rb) = (self.find(a), self.find(b));
        if ra != rb {
            self.parent.insert(ra.max(rb), ra.min(rb));
        }
    }
}

/// Median of a list (NaN-free); 0 for an empty one.
pub fn median(v: &mut Vec<f64>) -> f64 {
    if v.is_empty() {
        return 0.0;
    }
    v.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let n = v.len();
    if n % 2 == 1 {
        v[n / 2]
    } else {
        (v[n / 2 - 1] + v[n / 2]) / 2.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hull_of_a_square_with_an_inner_point() {
        let a = hull_area(&[[0.0, 0.0], [4.0, 0.0], [4.0, 3.0], [0.0, 3.0], [2.0, 1.0]]);
        assert!((a - 12.0).abs() < 1e-12);
    }

    #[test]
    fn colinear_points_have_no_hull_area() {
        assert_eq!(hull_area(&[[0.0, 0.0], [1.0, 0.0], [2.0, 0.0]]), 0.0);
    }

    #[test]
    fn quad_area() {
        let a = polygon_area(&[[0.0, 0.0, 1.0], [2.0, 0.0, 1.0], [2.0, 3.0, 1.0], [0.0, 3.0, 1.0]]);
        assert!((a - 6.0).abs() < 1e-12);
    }
}
