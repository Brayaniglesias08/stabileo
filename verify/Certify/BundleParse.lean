/-
JSON reader for certificate bundles (format `stabileo.linear-certificate.v1`, documented in
CERTIFY.md, "Independent checking: certificate bundles").

This reader is trusted (it is not verified): it maps the JSON to `Bundle` without any
arithmetic beyond decoding each number `[m, e]` to the rational `m · 2^e`, which is the
exact value of the f64 the engine wrote. `check_sound` is a statement about the decoded
`Bundle`.
-/
import Lean.Data.Json
import Certify.BundleCheck

namespace Certify.Bundle

open Lean (Json)

/-- `[m, e]` ↦ `m · 2^e`, exactly. -/
def decodeDyadic (j : Json) : Except String Rat := do
  let a ← j.getArr?
  unless a.size == 2 do throw s!"expected [mantissa, exponent], got {j.compress}"
  let m ← a[0]!.getInt?
  let e ← a[1]!.getInt?
  if e ≥ 0 then
    return (m * 2 ^ e.toNat : Int)
  else
    return mkRat m (2 ^ (-e).toNat)

def getField (j : Json) (k : String) : Except String Json :=
  match j.getObjVal? k with
  | .ok v => .ok v
  | .error _ => .error s!"missing field \"{k}\""

def natArray (j : Json) (k : String) : Except String (Array Nat) := do
  (← (← getField j k).getArr?).mapM (·.getNat?)

def ratArray (j : Json) (k : String) : Except String (Array Rat) := do
  (← (← getField j k).getArr?).mapM decodeDyadic

/-- A CSC matrix `{colPtr, rowIdx, values}` with `n` columns, as columns of `(row, value)`. -/
def cscColumns (j : Json) (n : Nat) : Except String (Array (List (Nat × Rat))) := do
  let cp ← natArray j "colPtr"
  let ri ← natArray j "rowIdx"
  let vs ← ratArray j "values"
  unless cp.size == n + 1 do throw s!"colPtr has length {cp.size}, expected {n + 1}"
  unless ri.size == vs.size do throw "rowIdx and values differ in length"
  unless cp[0]! == 0 && cp[n]! == ri.size do throw "colPtr does not span rowIdx"
  let mut cols : Array (List (Nat × Rat)) := Array.mkEmpty n
  for c in [0:n] do
    let (s, t) := (cp[c]!, cp[c + 1]!)
    unless s ≤ t && t ≤ ri.size do throw s!"colPtr not monotone at column {c}"
    let mut col : List (Nat × Rat) := []
    for p in [s:t] do
      col := (ri[p]!, vs[p]!) :: col
    cols := cols.push col.reverse
  return cols

def parseBundle (s : String) : Except String Bundle := do
  let j ← Json.parse s
  let fmt ← (← getField j "format").getStr?
  unless fmt == "stabileo.linear-certificate.v1" do throw s!"unsupported format {fmt}"
  let n ← (← getField j "n").getNat?
  let kCols ← cscColumns (← getField j "k") n
  let kEnt := (List.range n).flatMap fun c => (kCols[c]!).map fun (a, v) => (a, c, v)
  return {
    n
    kEnt
    fLo := ← ratArray j "fLo"
    fHi := ← ratArray j "fHi"
    u := ← ratArray j "u"
    perm := ← natArray j "perm"
    lCols := ← cscColumns (← getField j "l") n
    mu := ← decodeDyadic (← getField j "mu")
    bound := ← decodeDyadic (← getField j "bound")
  }

end Certify.Bundle
