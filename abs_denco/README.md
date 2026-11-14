# DENCO

It means Decode and Encode, but asynchronous.
Async version of `serde` of Rust, for serialization and deserialization.


## Optional features

The codec impls for standard containers are opt-in (except `str_alloc`). The
`type-id` feature adds a structural wire-type fingerprint:

| feature | effect |
| --- | --- |
| `type-id` | `abs_denco::type_id` with `TrHasTypeId` / `TrTypeId` / registry; must be paired with `denco_macro/type-id` |
| `type-id-murmur3` | use Murmur3 x86_32 / MurmurHash64A / x64_128 instead of FNV-1a 32/64/128 |

`TrHasTypeId` exposes `TYPE_ID_128` / `TYPE_ID_64` / `TYPE_ID_32`, each computed
with a native 32/64/128-bit hash and a width-specific domain (not a truncation of
the 128-bit value). The hash algorithm is part of the wire-compatibility contract;
pin exactly one hash feature per workspace. The trait implementation is generated
by the explicit `denco_macro::TrHasTypeId` derive when `denco_macro/type-id` is on.
