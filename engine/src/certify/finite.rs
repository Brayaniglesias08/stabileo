//! Finiteness guard shared by every certificate (issue #335).
//!
//! A certificate must never report success on a non-finite number: not in the
//! input it certifies, not in the reported results, not in its own bounds.
//! `f64::max`/`min` and comparisons silently drop NaN, so one NaN can turn
//! into an error bound of 0 (the spectral certificate did exactly that). The
//! checks here walk any `Serialize` value with a serde serializer, because
//! `serde_json` writes NaN and ±∞ as `null` and the check has to see the
//! values before they become JSON.
//!
//! The entry points use them as follows:
//! - `check_input`: a non-finite number anywhere in the input is an `Err`;
//! - `check_results`: a non-finite number in the solver's results is an `Err`
//!   (the solver should have refused; this is the backstop);
//! - `guard`: a certificate claiming success with a non-finite number outside
//!   `DOCUMENTED_INFINITE` is downgraded to uncertified.
//!
//! The fuzz harness (`engine/fuzz/src/lib.rs`) checks the same property with
//! the same walker and allowlist.

use serde::ser::{self, Serialize};
use std::fmt;

/// Fields whose documented values include +∞ (serialized as `null`):
/// - `relBound`: +∞ when ũ = 0 but the absolute bound is not 0;
/// - `omega2Upper`: +∞ when the residual is too large to bound ω² from above;
/// - `uniquenessRadius`: +∞ for a linear P-Delta model (β = 0);
/// - `bimoment` of a force bound: combinations and envelopes do not combine
///   the warping bimoment and report it unbounded (certify/combinations.rs);
/// - `b2Factor`: +∞ when the P-Delta solve fails or K + K_G is not positive
///   definite, reported with `isStable: false`.
///
/// Only +∞ is documented: a NaN is never allowed, in any field.
pub const DOCUMENTED_INFINITE: &[&str] = &["relBound", "omega2Upper", "uniquenessRadius", "bimoment", "b2Factor"];

/// Paths (`$.results.displacements[3].ux`) of every non-finite `f32`/`f64` in
/// `value`, with `NaN` or `inf` appended (`$.x=NaN`).
pub fn non_finite_paths<T: Serialize + ?Sized>(value: &T) -> Vec<String> {
    let mut a = Audit { path: vec!["$".to_string()], bad: Vec::new() };
    // Serialization errors (e.g. non-string map keys) only stop the walk.
    let _ = value.serialize(&mut a);
    a.bad.into_iter().map(|(p, x)| format!("{p}={x}")).collect()
}

/// Paths of the non-finite numbers in `value` that are not a documented +∞.
pub fn undocumented_non_finite<T: Serialize + ?Sized>(value: &T) -> Vec<String> {
    let mut a = Audit { path: vec!["$".to_string()], bad: Vec::new() };
    let _ = value.serialize(&mut a);
    a.bad
        .into_iter()
        .filter(|(p, x)| !(*x == f64::INFINITY && DOCUMENTED_INFINITE.iter().any(|f| p.ends_with(&format!(".{f}")))))
        .map(|(p, x)| format!("{p}={x}"))
        .collect()
}

fn describe(what: &str, bad: &[String]) -> String {
    let shown: Vec<&str> = bad.iter().take(3).map(String::as_str).collect();
    let more = if bad.len() > 3 { format!(" and {} more", bad.len() - 3) } else { String::new() };
    format!("non-finite number in the {what} at {}{more}", shown.join(", "))
}

/// `Err` if any number in the input is non-finite (no input field documents ±∞).
pub fn check_input<T: Serialize + ?Sized>(input: &T) -> Result<(), String> {
    let bad = non_finite_paths(input);
    if bad.is_empty() { Ok(()) } else { Err(describe("input", &bad)) }
}

/// `Err` if the results carry a non-finite number outside the documented +∞
/// fields.
pub fn check_results<T: Serialize + ?Sized>(results: &T) -> Result<(), String> {
    let bad = undocumented_non_finite(results);
    if bad.is_empty() { Ok(()) } else { Err(describe("results", &bad)) }
}

/// A certificate that can say whether it claims success and be downgraded.
pub trait Guarded: Serialize {
    /// Whether the certificate claims anything (certified, or certified in part).
    fn claims(&self) -> bool;
    /// Withdraw every claim, with the reason.
    fn downgrade(&mut self, reason: String);
}

/// Downgrade `cert` if it claims success while carrying an undocumented
/// non-finite number. Returns whether it was downgraded.
pub fn guard<C: Guarded + ?Sized>(cert: &mut C) -> bool {
    if !cert.claims() {
        return false;
    }
    let bad = undocumented_non_finite(cert);
    if bad.is_empty() {
        return false;
    }
    cert.downgrade(describe("certificate", &bad));
    true
}

/// The full guard of a certificate of given results: downgrade `cert` if the
/// input or the results it certifies carry a non-finite number, or if it
/// carries one itself (outside the documented +∞ fields).
pub fn guard_all<I, R, C>(input: &I, results: &R, cert: &mut C)
where
    I: Serialize + ?Sized,
    R: Serialize + ?Sized,
    C: Guarded + ?Sized,
{
    if !cert.claims() {
        return;
    }
    match check_input(input).and_then(|()| check_results(results)) {
        Err(e) => cert.downgrade(e),
        Ok(()) => {
            guard(cert);
        }
    }
}

/// `Guarded` for the enum certificates with `Certified { .. }` and
/// `Uncertified { reason }` variants.
macro_rules! guarded_enum {
    ($($t:ident),* $(,)?) => {$(
        impl $crate::certify::finite::Guarded for $t {
            fn claims(&self) -> bool {
                !matches!(self, $t::Uncertified { .. })
            }
            fn downgrade(&mut self, reason: String) {
                *self = $t::Uncertified { reason };
            }
        }
    )*};
}
pub(crate) use guarded_enum;

struct Audit {
    path: Vec<String>,
    bad: Vec<(String, f64)>,
}

#[derive(Debug)]
pub struct Error(String);

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for Error {}

impl ser::Error for Error {
    fn custom<T: fmt::Display>(msg: T) -> Self {
        Error(msg.to_string())
    }
}

impl Audit {
    fn float(&mut self, x: f64) {
        if !x.is_finite() {
            self.bad.push((self.path.concat(), x));
        }
    }
    fn nested<T: Serialize + ?Sized>(&mut self, seg: String, v: &T) -> Result<(), Error> {
        self.path.push(seg);
        let r = v.serialize(&mut *self);
        self.path.pop();
        r
    }
}

type R = Result<(), Error>;

impl<'a> ser::Serializer for &'a mut Audit {
    type Ok = ();
    type Error = Error;
    type SerializeSeq = Seq<'a>;
    type SerializeTuple = Seq<'a>;
    type SerializeTupleStruct = Seq<'a>;
    type SerializeTupleVariant = Seq<'a>;
    type SerializeMap = Map<'a>;
    type SerializeStruct = &'a mut Audit;
    type SerializeStructVariant = &'a mut Audit;

    fn serialize_bool(self, _: bool) -> R { Ok(()) }
    fn serialize_i8(self, _: i8) -> R { Ok(()) }
    fn serialize_i16(self, _: i16) -> R { Ok(()) }
    fn serialize_i32(self, _: i32) -> R { Ok(()) }
    fn serialize_i64(self, _: i64) -> R { Ok(()) }
    fn serialize_u8(self, _: u8) -> R { Ok(()) }
    fn serialize_u16(self, _: u16) -> R { Ok(()) }
    fn serialize_u32(self, _: u32) -> R { Ok(()) }
    fn serialize_u64(self, _: u64) -> R { Ok(()) }
    fn serialize_f32(self, v: f32) -> R {
        self.float(v as f64);
        Ok(())
    }
    fn serialize_f64(self, v: f64) -> R {
        self.float(v);
        Ok(())
    }
    fn serialize_char(self, _: char) -> R { Ok(()) }
    fn serialize_str(self, _: &str) -> R { Ok(()) }
    fn serialize_bytes(self, _: &[u8]) -> R { Ok(()) }
    fn serialize_none(self) -> R { Ok(()) }
    fn serialize_some<T: Serialize + ?Sized>(self, v: &T) -> R { v.serialize(self) }
    fn serialize_unit(self) -> R { Ok(()) }
    fn serialize_unit_struct(self, _: &'static str) -> R { Ok(()) }
    fn serialize_unit_variant(self, _: &'static str, _: u32, _: &'static str) -> R { Ok(()) }
    fn serialize_newtype_struct<T: Serialize + ?Sized>(self, _: &'static str, v: &T) -> R {
        v.serialize(self)
    }
    fn serialize_newtype_variant<T: Serialize + ?Sized>(
        self,
        _: &'static str,
        _: u32,
        variant: &'static str,
        v: &T,
    ) -> R {
        self.nested(format!(".{variant}"), v)
    }
    fn serialize_seq(self, _: Option<usize>) -> Result<Seq<'a>, Error> {
        Ok(Seq { a: self, i: 0 })
    }
    fn serialize_tuple(self, _: usize) -> Result<Seq<'a>, Error> {
        Ok(Seq { a: self, i: 0 })
    }
    fn serialize_tuple_struct(self, _: &'static str, _: usize) -> Result<Seq<'a>, Error> {
        Ok(Seq { a: self, i: 0 })
    }
    fn serialize_tuple_variant(self, _: &'static str, _: u32, _: &'static str, _: usize) -> Result<Seq<'a>, Error> {
        Ok(Seq { a: self, i: 0 })
    }
    fn serialize_map(self, _: Option<usize>) -> Result<Map<'a>, Error> {
        Ok(Map { a: self, key: String::new() })
    }
    fn serialize_struct(self, _: &'static str, _: usize) -> Result<&'a mut Audit, Error> {
        Ok(self)
    }
    fn serialize_struct_variant(
        self,
        _: &'static str,
        _: u32,
        _: &'static str,
        _: usize,
    ) -> Result<&'a mut Audit, Error> {
        Ok(self)
    }
}

pub struct Seq<'a> {
    a: &'a mut Audit,
    i: usize,
}

impl Seq<'_> {
    fn elem<T: Serialize + ?Sized>(&mut self, v: &T) -> R {
        let seg = format!("[{}]", self.i);
        self.i += 1;
        self.a.nested(seg, v)
    }
}

impl ser::SerializeSeq for Seq<'_> {
    type Ok = ();
    type Error = Error;
    fn serialize_element<T: Serialize + ?Sized>(&mut self, v: &T) -> R { self.elem(v) }
    fn end(self) -> R { Ok(()) }
}

impl ser::SerializeTuple for Seq<'_> {
    type Ok = ();
    type Error = Error;
    fn serialize_element<T: Serialize + ?Sized>(&mut self, v: &T) -> R { self.elem(v) }
    fn end(self) -> R { Ok(()) }
}

impl ser::SerializeTupleStruct for Seq<'_> {
    type Ok = ();
    type Error = Error;
    fn serialize_field<T: Serialize + ?Sized>(&mut self, v: &T) -> R { self.elem(v) }
    fn end(self) -> R { Ok(()) }
}

impl ser::SerializeTupleVariant for Seq<'_> {
    type Ok = ();
    type Error = Error;
    fn serialize_field<T: Serialize + ?Sized>(&mut self, v: &T) -> R { self.elem(v) }
    fn end(self) -> R { Ok(()) }
}

pub struct Map<'a> {
    a: &'a mut Audit,
    key: String,
}

impl ser::SerializeMap for Map<'_> {
    type Ok = ();
    type Error = Error;
    fn serialize_key<T: Serialize + ?Sized>(&mut self, k: &T) -> R {
        self.key = serde_json::to_value(k)
            .map(|v| v.as_str().map(str::to_string).unwrap_or_else(|| v.to_string()))
            .unwrap_or_default();
        Ok(())
    }
    fn serialize_value<T: Serialize + ?Sized>(&mut self, v: &T) -> R {
        let seg = format!("[{:?}]", self.key);
        self.a.nested(seg, v)
    }
    fn end(self) -> R { Ok(()) }
}

impl ser::SerializeStruct for &mut Audit {
    type Ok = ();
    type Error = Error;
    fn serialize_field<T: Serialize + ?Sized>(&mut self, key: &'static str, v: &T) -> R {
        self.nested(format!(".{key}"), v)
    }
    fn end(self) -> R { Ok(()) }
}

impl ser::SerializeStructVariant for &mut Audit {
    type Ok = ();
    type Error = Error;
    fn serialize_field<T: Serialize + ?Sized>(&mut self, key: &'static str, v: &T) -> R {
        self.nested(format!(".{key}"), v)
    }
    fn end(self) -> R { Ok(()) }
}
