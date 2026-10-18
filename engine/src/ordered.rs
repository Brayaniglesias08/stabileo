//! Deterministic iteration over the solver inputs' maps.
//!
//! Solver inputs keep nodes, elements, supports, … in `HashMap<String, _>`, and every
//! `HashMap` gets its own random hash keys: the same JSON parsed twice iterates in two
//! different orders. A floating-point sum taken in that order (an assembled stiffness
//! entry, a load vector, an equilibrium total) can then differ in the last bit between two
//! solves of the same model, and results listed in that order come out shuffled.
//!
//! Every loop over an input map goes through [`values`], which yields the entries in key
//! order, so a model always solves to the same bits (tests/determinism.rs;
//! docs/verification/PROVENANCE.md, "Determinism").

/// The map's values in increasing key order.
pub fn values<'a, K: Ord + 'a, V: 'a, M: ?Sized>(m: &'a M) -> std::vec::IntoIter<&'a V>
where
    &'a M: IntoIterator<Item = (&'a K, &'a V)>,
{
    let mut v: Vec<(&'a K, &'a V)> = m.into_iter().collect();
    v.sort_unstable_by(|a, b| a.0.cmp(b.0));
    v.into_iter().map(|(_, x)| x).collect::<Vec<_>>().into_iter()
}
