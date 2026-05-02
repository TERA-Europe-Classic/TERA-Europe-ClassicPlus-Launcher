// Task 1 of the x32→x64 transformer pipeline.  The public API is consumed by
// Task 5 (the transformer itself) which is not yet written.  Until then, allow
// dead-code lints rather than force-wiring an incomplete pipeline.
#![allow(dead_code)]

//! UE3 property-block parser for TERA GPK exports.
//!
//! Handles the two binary layouts that differ between 32-bit (Classic,
//! FileVersion 610) and 64-bit (v100.02, FileVersion 897) packages:
//!
//! - **BoolProperty**: 4 bytes on x32, 1 byte on x64.
//! - **ByteProperty**: no `enumType` name-index prefix on x32; 8-byte prefix
//!   on x64.
//!
//! All other property types are identical across arches.
//!
//! # Usage
//!
//! ```ignore
//! use app::services::mods::gpk_property::{parse_properties, ArchKind};
//!
//! let props = parse_properties(&export.payload[4..], ArchKind::X32, &pkg.names)?;
//! ```
//!
//! The caller is responsible for stripping the 4-byte NetIndex prefix that
//! precedes the property block inside every export payload.

use super::gpk_package::GpkNameEntry;

// ── Public types ─────────────────────────────────────────────────────────────

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ArchKind {
    X32,
    X64,
}

#[derive(Clone, Debug)]
pub struct Property {
    pub name: String,
    pub type_name: String,
    pub array_index: i32,
    pub value: PropertyValue,
}

#[derive(Clone, Debug)]
pub enum PropertyValue {
    Int(i32),
    Float(f32),
    Bool(bool),
    /// `enum_type`: `None` on x32 (no prefix), `Some(name)` on x64 (name at
    ///   index 0 means "no enum type").
    /// `value`: the raw byte value (present when `header_size == 1`).
    /// `name_value`: name-table entry used when `header_size != 1`.
    Byte {
        enum_type: Option<String>,
        value: u8,
        name_value: Option<String>,
    },
    Name(String),
    Object(i32),
    Str(String),
    Struct {
        inner_type: String,
        raw: Vec<u8>,
    },
    Array(Vec<u8>),
    None,
}

// ── Parser ────────────────────────────────────────────────────────────────────

/// Parse a UE3 property block from `bytes`.
///
/// `bytes` must start at the first property header — the caller must strip the
/// 4-byte NetIndex prefix that precedes the block inside an export payload.
///
/// Returns a `Vec` that always ends with a `Property { name: "None", … }`.
pub fn parse_properties(
    bytes: &[u8],
    arch: ArchKind,
    names: &[GpkNameEntry],
) -> Result<Vec<Property>, String> {
    let mut cursor = 0usize;
    let mut props = Vec::new();

    loop {
        // Each header is 24 bytes: i64 name_idx | i64 type_idx | i32 size | i32 array_index.
        let name_idx = read_u64(bytes, &mut cursor)?;
        let name = lookup_name(names, name_idx)?;

        if name == "None" {
            props.push(Property {
                name: "None".into(),
                type_name: "None".into(),
                array_index: 0,
                value: PropertyValue::None,
            });
            return Ok(props);
        }

        let type_idx = read_u64(bytes, &mut cursor)?;
        let type_name = lookup_name(names, type_idx)?;
        let size = read_i32(bytes, &mut cursor)? as usize;
        let array_index = read_i32(bytes, &mut cursor)?;

        let value = parse_value(bytes, &mut cursor, &type_name, size, arch, names)?;

        props.push(Property {
            name,
            type_name,
            array_index,
            value,
        });
    }
}

// ── Value dispatch ────────────────────────────────────────────────────────────

fn parse_value(
    bytes: &[u8],
    cursor: &mut usize,
    type_name: &str,
    size: usize,
    arch: ArchKind,
    names: &[GpkNameEntry],
) -> Result<PropertyValue, String> {
    match type_name {
        "IntProperty" => {
            let v = read_i32(bytes, cursor)?;
            Ok(PropertyValue::Int(v))
        }
        "FloatProperty" => {
            let v = read_f32(bytes, cursor)?;
            Ok(PropertyValue::Float(v))
        }
        "BoolProperty" => {
            // header `size` field is 0 for BoolProperty; actual value width is
            // arch-dependent: 4 bytes on x32, 1 byte on x64.
            let v = match arch {
                ArchKind::X32 => {
                    let raw = read_i32(bytes, cursor)?;
                    raw != 0
                }
                ArchKind::X64 => {
                    let raw = read_u8(bytes, cursor)?;
                    raw != 0
                }
            };
            Ok(PropertyValue::Bool(v))
        }
        "ByteProperty" => parse_byte_property(bytes, cursor, size, arch, names),
        "NameProperty" => {
            // i32 name index + 4 bytes padding (8 total).
            let idx = read_i32(bytes, cursor)? as usize;
            let _pad = read_u32(bytes, cursor)?;
            let name = names
                .get(idx)
                .map(|e| e.name.clone())
                .ok_or_else(|| format!("NameProperty: name index {idx} out of range"))?;
            Ok(PropertyValue::Name(name))
        }
        "ObjectProperty" => {
            let v = read_i32(bytes, cursor)?;
            Ok(PropertyValue::Object(v))
        }
        "StrProperty" => {
            let s = read_fstring(bytes, cursor)?;
            Ok(PropertyValue::Str(s))
        }
        "StructProperty" => {
            let inner_idx = read_u64(bytes, cursor)?;
            let inner_type = lookup_name(names, inner_idx)?;
            let raw = read_bytes(bytes, cursor, size)?;
            Ok(PropertyValue::Struct { inner_type, raw })
        }
        "ArrayProperty" => {
            let raw = read_bytes(bytes, cursor, size)?;
            Ok(PropertyValue::Array(raw))
        }
        other => Err(format!("unsupported property type {other:?}")),
    }
}

fn parse_byte_property(
    bytes: &[u8],
    cursor: &mut usize,
    size: usize,
    arch: ArchKind,
    names: &[GpkNameEntry],
) -> Result<PropertyValue, String> {
    // x64 has an 8-byte enumType name-index prefix; x32 does not.
    let enum_type = match arch {
        ArchKind::X64 => {
            let idx = read_u64(bytes, cursor)?;
            let name = lookup_name(names, idx)?;
            // Index 0 conventionally means "no enum"; keep the name for
            // fidelity but callers may treat an empty/"None" name as absent.
            Some(name)
        }
        ArchKind::X32 => None,
    };

    // If header `size` == 1 the value is a raw byte; otherwise it is an 8-byte
    // name-table index pointing at the enum member name.
    if size == 1 {
        let v = read_u8(bytes, cursor)?;
        Ok(PropertyValue::Byte {
            enum_type,
            value: v,
            name_value: None,
        })
    } else {
        let idx = read_u64(bytes, cursor)?;
        let name = lookup_name(names, idx)?;
        Ok(PropertyValue::Byte {
            enum_type,
            value: 0,
            name_value: Some(name),
        })
    }
}

// ── Writer ────────────────────────────────────────────────────────────────────

/// Write a UE3 property block into `out`.
///
/// `props` must end with a `Property { name: "None", … }` terminator (as
/// returned by `parse_properties`).  Returns `Err` if the terminator is absent
/// or if any name cannot be found in `names`.
pub fn write_properties(
    props: &[Property],
    arch: ArchKind,
    names: &[GpkNameEntry],
    out: &mut Vec<u8>,
) -> Result<(), String> {
    let mut found_none = false;
    for p in props {
        let name_idx = encode_name_index(names, &p.name)?;
        out.extend_from_slice(&name_idx.to_le_bytes());

        if p.name == "None" {
            found_none = true;
            // None terminator: only the 8-byte name index; nothing else follows.
            break;
        }

        let type_idx = encode_name_index(names, &p.type_name)?;
        out.extend_from_slice(&type_idx.to_le_bytes());

        let size = compute_value_size(&p.value, arch)? as i32;
        out.extend_from_slice(&size.to_le_bytes());
        out.extend_from_slice(&p.array_index.to_le_bytes());

        write_value(&p.value, arch, names, out)?;
    }

    if !found_none {
        return Err("property block missing None terminator".into());
    }
    Ok(())
}

/// Returns the value the parser sees in the on-disk `size` header field.
///
/// This is NOT the total bytes emitted by `write_value` — it excludes the
/// arch-dependent overhead that the parser reads outside the size accounting
/// (BoolProperty width, StructProperty inner_type prefix, x64 ByteProperty
/// enum_type prefix).
fn compute_value_size(value: &PropertyValue, _arch: ArchKind) -> Result<usize, String> {
    let n = match value {
        PropertyValue::Int(_) | PropertyValue::Float(_) | PropertyValue::Object(_) => 4,
        PropertyValue::Bool(_) => 0,
        PropertyValue::Byte {
            name_value: Some(_),
            ..
        } => 8,
        PropertyValue::Byte {
            name_value: None, ..
        } => 1,
        PropertyValue::Name(_) => 8,
        PropertyValue::Str(s) => {
            if !s.is_ascii() {
                return Err(format!(
                    "StrProperty: non-ASCII string {s:?} is not supported"
                ));
            }
            4 + s.len() + 1
        }
        PropertyValue::Struct { raw, .. } => raw.len(),
        PropertyValue::Array(raw) => raw.len(),
        PropertyValue::None => 0,
    };
    Ok(n)
}

/// Emit value bytes — exact inverse of `parse_value`.
fn write_value(
    value: &PropertyValue,
    arch: ArchKind,
    names: &[GpkNameEntry],
    out: &mut Vec<u8>,
) -> Result<(), String> {
    match value {
        PropertyValue::Int(v) => out.extend_from_slice(&v.to_le_bytes()),
        PropertyValue::Float(v) => out.extend_from_slice(&v.to_le_bytes()),
        PropertyValue::Bool(v) => match arch {
            ArchKind::X32 => out.extend_from_slice(&(*v as i32).to_le_bytes()),
            ArchKind::X64 => out.push(*v as u8),
        },
        PropertyValue::Byte {
            enum_type,
            value: raw_byte,
            name_value,
        } => {
            // x64 only: 8-byte enum_type name-index prefix.
            if arch == ArchKind::X64 {
                let et_name = enum_type.as_deref().unwrap_or("None");
                // An empty string or "None" maps to index 0 which the parser
                // treats as "no enum type".
                let et_idx = if et_name.is_empty() || et_name == "None" {
                    0i64
                } else {
                    encode_name_index(names, et_name)?
                };
                out.extend_from_slice(&et_idx.to_le_bytes());
            }
            // Value: either 8-byte name index or raw byte.
            if let Some(nv) = name_value {
                let nv_idx = encode_name_index(names, nv)?;
                out.extend_from_slice(&nv_idx.to_le_bytes());
            } else {
                out.push(*raw_byte);
            }
        }
        PropertyValue::Name(s) => {
            let idx = encode_name_index(names, s)?;
            // i32 (lower 32 bits) + u32 padding 0 = 8 bytes total.
            out.extend_from_slice(&(idx as i32).to_le_bytes());
            out.extend_from_slice(&0u32.to_le_bytes());
        }
        PropertyValue::Object(v) => out.extend_from_slice(&v.to_le_bytes()),
        PropertyValue::Str(s) => {
            if !s.is_ascii() {
                return Err(format!(
                    "StrProperty: non-ASCII string {s:?} is not supported"
                ));
            }
            let len = s.len() as i32 + 1; // +1 for null terminator
            out.extend_from_slice(&len.to_le_bytes());
            out.extend_from_slice(s.as_bytes());
            out.push(0u8); // null terminator
        }
        PropertyValue::Struct { inner_type, raw } => {
            let it_idx = encode_name_index(names, inner_type)?;
            out.extend_from_slice(&it_idx.to_le_bytes());
            out.extend_from_slice(raw);
        }
        PropertyValue::Array(raw) => out.extend_from_slice(raw),
        PropertyValue::None => {
            // Handled by write_properties directly; should not be reached.
        }
    }
    Ok(())
}

/// Encode a name-table entry as an i64 for on-disk storage.
///
/// Lower 32 bits = the index into `names`; upper 32 bits = 0 (no numeric
/// suffix in our data).
fn encode_name_index(names: &[GpkNameEntry], wanted: &str) -> Result<i64, String> {
    let idx = names
        .iter()
        .position(|e| e.name == wanted)
        .ok_or_else(|| format!("name '{wanted}' not in name table"))?;
    Ok(idx as i64)
}

// ── Low-level read helpers ────────────────────────────────────────────────────

fn read_slice<'a>(bytes: &'a [u8], cursor: &mut usize, len: usize) -> Result<&'a [u8], String> {
    let end = cursor
        .checked_add(len)
        .ok_or("property parser: cursor overflow")?;
    if end > bytes.len() {
        return Err(format!(
            "property parser: unexpected EOF at {cursor}+{len} (buffer len {})",
            bytes.len()
        ));
    }
    let slice = &bytes[*cursor..end];
    *cursor = end;
    Ok(slice)
}

fn read_u8(bytes: &[u8], cursor: &mut usize) -> Result<u8, String> {
    let s = read_slice(bytes, cursor, 1)?;
    Ok(s[0])
}

fn read_u32(bytes: &[u8], cursor: &mut usize) -> Result<u32, String> {
    let s = read_slice(bytes, cursor, 4)?;
    Ok(u32::from_le_bytes([s[0], s[1], s[2], s[3]]))
}

fn read_i32(bytes: &[u8], cursor: &mut usize) -> Result<i32, String> {
    let s = read_slice(bytes, cursor, 4)?;
    Ok(i32::from_le_bytes([s[0], s[1], s[2], s[3]]))
}

fn read_f32(bytes: &[u8], cursor: &mut usize) -> Result<f32, String> {
    let s = read_slice(bytes, cursor, 4)?;
    Ok(f32::from_le_bytes([s[0], s[1], s[2], s[3]]))
}

fn read_u64(bytes: &[u8], cursor: &mut usize) -> Result<u64, String> {
    let s = read_slice(bytes, cursor, 8)?;
    Ok(u64::from_le_bytes([
        s[0], s[1], s[2], s[3], s[4], s[5], s[6], s[7],
    ]))
}

fn read_fstring(bytes: &[u8], cursor: &mut usize) -> Result<String, String> {
    let len = read_i32(bytes, cursor)?;
    if len == 0 {
        return Ok(String::new());
    }
    if len > 0 {
        let data = read_slice(bytes, cursor, len as usize)?;
        let without_null = data.strip_suffix(&[0u8]).unwrap_or(data);
        return Ok(String::from_utf8_lossy(without_null).into_owned());
    }
    // Negative length → UTF-16 LE, length is in UTF-16 code units.
    Err(format!(
        "StrProperty: UTF-16 strings (length {len}) are not supported; only ASCII mods are expected"
    ))
}

fn read_bytes(bytes: &[u8], cursor: &mut usize, len: usize) -> Result<Vec<u8>, String> {
    let s = read_slice(bytes, cursor, len)?;
    Ok(s.to_vec())
}

// ── Name lookup ───────────────────────────────────────────────────────────────

/// Resolve a name from an encoded 64-bit index. The lower 32 bits are the
/// table index; the upper 32 bits are an unused numeric suffix.
fn lookup_name(names: &[GpkNameEntry], encoded: u64) -> Result<String, String> {
    let idx = (encoded & 0xFFFF_FFFF) as usize;
    names
        .get(idx)
        .map(|e| e.name.clone())
        .ok_or_else(|| format!("property parser: name index {idx} out of range"))
}
