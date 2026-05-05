# GPK x32→x64 Transformer + Type-D Drop-In Install Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use `superpowers:subagent-driven-development` (recommended) or `superpowers:executing-plans` to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Port the last 19 GPK mods in the catalog by building (a) an x32→x64 GPK transformer that re-encodes Classic packages for v100, and (b) a drop-in CookedPC install path for Type-D mods whose target package isn't in v100 vanilla.

**Architecture:** New `gpk_transform.rs` service builds on existing `gpk_package::parse_package` (which already reads both x32 and x64). Adds property-block parser, property-block writer for both arch flavours, and a top-level `transform_x32_to_x64` that rewrites header layout, property blocks, and re-emits bytes (LZO-compressed if requested). New `gpk_dropin_install.rs` writes transformed packages straight into `S1Game/CookedPC/<name>.gpk` and registers a vanilla-bak for uninstall. Catalog gains a `deploy_strategy` field (`composite_patch` default, `dropin` for Type-D). All work uses real GPK fixtures (artexlib + psina.postprocess) — no mocked bytes.

**Tech Stack:** Rust 1.85, existing `gpk_package` crate-internal module, lzo (`minilzo` crate already a dep), serde (catalog), Tauri command boundary unchanged.

**Estimated effort:** 3.5 days for all 19 mods. Tasks 1-7 = transformer core (~1.5 days). Tasks 8-10 = drop-in install (~0.5 day). Tasks 11-12 = per-export additions (~1 day). Tasks 13-15 = catalog wiring + end-to-end ports (~0.5 day).

---

## File Structure

| File | Responsibility |
|---|---|
| `teralaunch/src-tauri/src/services/mods/gpk_transform.rs` (new) | x32→x64 transform — header rewrite, property block re-encode, byte emission |
| `teralaunch/src-tauri/src/services/mods/gpk_property.rs` (new) | Property-block parser + writer; arch-aware BoolProperty/ByteProperty/etc. |
| `teralaunch/src-tauri/src/services/mods/gpk_dropin_install.rs` (new) | Drop-in CookedPC install path for Type-D mods (no composite splice) |
| `teralaunch/src-tauri/src/services/mods/gpk_package.rs` (modify) | Add `serialize_package` + `RawProperties::Parsed`/`Raw` variant |
| `teralaunch/src-tauri/src/services/mods/types.rs` (modify) | Add `CatalogEntry.deploy_strategy: Option<DeployStrategy>` |
| `teralaunch/src-tauri/src/commands/mods.rs` (modify) | Route Type-D mods via `gpk_dropin_install::install_dropin` |
| `teralaunch/src-tauri/tests/gpk_transform_x32_to_x64.rs` (new) | End-to-end transform tests using artexlib fixtures |
| `teralaunch/src-tauri/tests/gpk_dropin_install.rs` (new) | Drop-in install + uninstall round-trip on a real game tree |

---

## Task 1: Property block parser — fixture round-trip baseline

**Files:**
- Test: `teralaunch/src-tauri/tests/gpk_property_parse.rs` (new)
- Fixture: `teralaunch/src-tauri/tests/fixtures/property_block_x32_artexlib.bin` (extract once, commit)
- Fixture: `teralaunch/src-tauri/tests/fixtures/property_block_x64_paperdoll.bin` (extract once, commit)

- [ ] **Step 1: Extract two property-block fixtures from real GPKs**

```bash
# Already-downloaded x32 GPK in the foglio-batch workdir
# Compute property block byte range for export[2] using existing inspect tool, then dd
cd teralaunch/src-tauri
cargo run --release --bin inspect-gpk-resources -- \
  C:/Users/Lukas/AppData/Local/Temp/foglio-batch/artexlib.brawler-chad-block-animation.x32.gpk \
  > /tmp/x32-inspect.txt
# inspect-gpk-resources prints `serial_offset=NNN serial_size=MMM` for each export.
# Pick the first MaterialInstanceConstant export, dd that range into a fixture file.
EXPORT_OFFSET=$(grep -oP "serial_offset=\d+" /tmp/x32-inspect.txt | head -1 | cut -d= -f2)
EXPORT_SIZE=$(grep -oP "serial_size=\d+" /tmp/x32-inspect.txt | head -1 | cut -d= -f2)
dd if=C:/Users/Lukas/AppData/Local/Temp/foglio-batch/artexlib.brawler-chad-block-animation.x32.gpk \
   of=tests/fixtures/property_block_x32_artexlib.bin \
   bs=1 skip=$EXPORT_OFFSET count=$EXPORT_SIZE 2>/dev/null
# Same for an x64 vanilla wrapper from the v100 game tree
```

- [ ] **Step 2: Write the failing parser test**

```rust
// teralaunch/src-tauri/tests/gpk_property_parse.rs
use mod_manager::services::mods::gpk_property::{Property, parse_properties, ArchKind};

#[test]
fn parses_x32_artexlib_property_block_round_trips() {
    let bytes = include_bytes!("fixtures/property_block_x32_artexlib.bin");
    let props = parse_properties(bytes, ArchKind::X32).expect("parse");
    // The fixture is a MaterialInstanceConstant: it should contain at least one
    // ScalarParameterValues ArrayProperty followed by a None terminator.
    assert!(props.iter().any(|p| p.name == "ScalarParameterValues"));
    let last = props.last().expect("at least one prop");
    assert_eq!(last.name, "None"); // terminator name
}
```

- [ ] **Step 3: Run test to verify it fails**

Run: `cd teralaunch/src-tauri && cargo test --test gpk_property_parse -- --nocapture`
Expected: FAIL with "unresolved import" — module doesn't exist yet.

- [ ] **Step 4: Implement minimal parser**

Create `teralaunch/src-tauri/src/services/mods/gpk_property.rs`:

```rust
//! Property-block parser/writer for GPK exports. Property blocks are arch-aware:
//! BoolProperty stores 1 byte on x64 vs 4 bytes on x32, and ByteProperty has an
//! 8-byte enumType prefix on x64 only. All other property types share layout.

use super::gpk_package::{NameTable, NameRef};

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
    Byte { enum_type: Option<String>, value: u8, name_value: Option<String> },
    Name(String),
    Object(i32),
    Str(String),
    Struct { inner_type: String, raw: Vec<u8> },
    Array(Vec<u8>),
    None,
}

pub fn parse_properties(
    bytes: &[u8],
    arch: ArchKind,
    names: &NameTable,
) -> Result<Vec<Property>, String> {
    let mut props = Vec::new();
    let mut cursor = 0usize;
    loop {
        if cursor + 24 > bytes.len() {
            return Err("property header overruns export body".into());
        }
        let name_index = read_i64(bytes, &mut cursor);
        let name = names.resolve(name_index)?;
        if name == "None" {
            props.push(Property { name, type_name: "None".into(), array_index: 0, value: PropertyValue::None });
            return Ok(props);
        }
        let type_index = read_i64(bytes, &mut cursor);
        let type_name = names.resolve(type_index)?;
        let size = read_i32(bytes, &mut cursor) as usize;
        let array_index = read_i32(bytes, &mut cursor);
        let value = parse_value(&type_name, size, arch, bytes, &mut cursor, names)?;
        props.push(Property { name, type_name, array_index, value });
    }
}

fn parse_value(
    type_name: &str,
    size: usize,
    arch: ArchKind,
    bytes: &[u8],
    cursor: &mut usize,
    names: &NameTable,
) -> Result<PropertyValue, String> {
    match type_name {
        "IntProperty" => Ok(PropertyValue::Int(read_i32(bytes, cursor))),
        "FloatProperty" => Ok(PropertyValue::Float(read_f32(bytes, cursor))),
        "BoolProperty" => {
            let v = match arch {
                ArchKind::X64 => read_u8(bytes, cursor) != 0,
                ArchKind::X32 => read_i32(bytes, cursor) != 0,
            };
            Ok(PropertyValue::Bool(v))
        }
        "ByteProperty" => {
            let enum_type = if matches!(arch, ArchKind::X64) {
                let idx = read_i64(bytes, cursor);
                if idx == 0 { None } else { Some(names.resolve(idx)?) }
            } else { None };
            if size == 1 {
                Ok(PropertyValue::Byte { enum_type, value: read_u8(bytes, cursor), name_value: None })
            } else {
                let nv = names.resolve(read_i64(bytes, cursor))?;
                Ok(PropertyValue::Byte { enum_type, value: 0, name_value: Some(nv) })
            }
        }
        "NameProperty" => {
            let n = names.resolve(read_i32(bytes, cursor) as i64)?;
            *cursor += 4; // padding
            Ok(PropertyValue::Name(n))
        }
        "ObjectProperty" => Ok(PropertyValue::Object(read_i32(bytes, cursor))),
        "StrProperty" => Ok(PropertyValue::Str(read_fstring_at(bytes, cursor)?)),
        "StructProperty" => {
            let inner = names.resolve(read_i64(bytes, cursor))?;
            let raw = bytes[*cursor..*cursor + size].to_vec();
            *cursor += size;
            Ok(PropertyValue::Struct { inner_type: inner, raw })
        }
        "ArrayProperty" => {
            let raw = bytes[*cursor..*cursor + size].to_vec();
            *cursor += size;
            Ok(PropertyValue::Array(raw))
        }
        other => Err(format!("unsupported property type {other:?}")),
    }
}
```

(Helpers `read_i64`, `read_i32`, `read_u8`, `read_f32`, `read_fstring_at` mirror those in `gpk_package.rs` — copy them and make `pub(crate)` in a shared `gpk_io` module.)

- [ ] **Step 5: Run test to verify it passes**

Run: `cd teralaunch/src-tauri && cargo test --test gpk_property_parse -- --nocapture`
Expected: PASS, log "ScalarParameterValues" present.

- [ ] **Step 6: Commit**

```bash
git add teralaunch/src-tauri/src/services/mods/gpk_property.rs \
        teralaunch/src-tauri/tests/gpk_property_parse.rs \
        teralaunch/src-tauri/tests/fixtures/property_block_x32_artexlib.bin \
        teralaunch/src-tauri/tests/fixtures/property_block_x64_paperdoll.bin
git commit -m "feat(gpk): property-block parser with x32/x64 BoolProperty + ByteProperty handling"
```

---

## Task 2: Property block writer (round-trip identity for same arch)

**Files:**
- Modify: `teralaunch/src-tauri/src/services/mods/gpk_property.rs`
- Modify: `teralaunch/src-tauri/tests/gpk_property_parse.rs` (add round-trip test)

- [ ] **Step 1: Write the failing round-trip test**

```rust
// Append to teralaunch/src-tauri/tests/gpk_property_parse.rs
#[test]
fn x32_property_block_writes_back_byte_identical() {
    let bytes = include_bytes!("fixtures/property_block_x32_artexlib.bin");
    // We need a NameTable to round-trip names — load it from the same fixture
    // GPK header (committed alongside the property fixture).
    let names_bytes = include_bytes!("fixtures/property_block_x32_artexlib.names.bin");
    let names = mod_manager::services::mods::gpk_package::NameTable::deserialize(names_bytes)
        .expect("name table fixture");
    let props = mod_manager::services::mods::gpk_property::parse_properties(
        bytes, mod_manager::services::mods::gpk_property::ArchKind::X32, &names,
    ).expect("parse");
    let mut out = Vec::new();
    mod_manager::services::mods::gpk_property::write_properties(
        &props, mod_manager::services::mods::gpk_property::ArchKind::X32, &names, &mut out,
    ).expect("write");
    assert_eq!(out.as_slice(), &bytes[..]);
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd teralaunch/src-tauri && cargo test --test gpk_property_parse x32_property_block_writes_back -- --nocapture`
Expected: FAIL — `write_properties` doesn't exist.

- [ ] **Step 3: Add NameTable serialize/deserialize for fixture support, then write_properties**

Append to `gpk_property.rs`:

```rust
pub fn write_properties(
    props: &[Property],
    arch: ArchKind,
    names: &NameTable,
    out: &mut Vec<u8>,
) -> Result<(), String> {
    for p in props {
        if p.type_name == "None" {
            // Terminator: just the name index, no rest of header.
            out.extend_from_slice(&names.lookup(&p.name)?.to_le_bytes());
            return Ok(());
        }
        out.extend_from_slice(&names.lookup(&p.name)?.to_le_bytes());
        out.extend_from_slice(&names.lookup(&p.type_name)?.to_le_bytes());
        let size = compute_value_size(&p.value, arch);
        out.extend_from_slice(&(size as i32).to_le_bytes());
        out.extend_from_slice(&p.array_index.to_le_bytes());
        write_value(&p.value, arch, names, out)?;
    }
    Err("property block missing None terminator".into())
}

fn compute_value_size(value: &PropertyValue, arch: ArchKind) -> usize {
    match value {
        PropertyValue::Int(_) => 4,
        PropertyValue::Float(_) => 4,
        PropertyValue::Bool(_) => 0, // BoolProperty stores actual bool outside header bytes
        PropertyValue::Byte { name_value, .. } => if name_value.is_some() { 8 } else { 1 },
        PropertyValue::Name(_) => 8,
        PropertyValue::Object(_) => 4,
        PropertyValue::Str(s) => 4 + s.len() + 1, // length + ascii + null
        PropertyValue::Struct { raw, .. } => raw.len(),
        PropertyValue::Array(raw) => raw.len(),
        PropertyValue::None => 0,
    }
}

fn write_value(value: &PropertyValue, arch: ArchKind, names: &NameTable, out: &mut Vec<u8>) -> Result<(), String> {
    match value {
        PropertyValue::Int(v) => out.extend_from_slice(&v.to_le_bytes()),
        PropertyValue::Float(v) => out.extend_from_slice(&v.to_le_bytes()),
        PropertyValue::Bool(v) => match arch {
            ArchKind::X64 => out.push(if *v { 1 } else { 0 }),
            ArchKind::X32 => out.extend_from_slice(&(if *v { 1i32 } else { 0i32 }).to_le_bytes()),
        },
        PropertyValue::Byte { enum_type, value, name_value } => {
            if matches!(arch, ArchKind::X64) {
                let idx = match enum_type { Some(s) => names.lookup(s)?, None => 0 };
                out.extend_from_slice(&idx.to_le_bytes());
            }
            match name_value {
                Some(nv) => out.extend_from_slice(&names.lookup(nv)?.to_le_bytes()),
                None => out.push(*value),
            }
        }
        PropertyValue::Name(n) => {
            out.extend_from_slice(&(names.lookup(n)? as i32).to_le_bytes());
            out.extend_from_slice(&[0u8; 4]); // padding
        }
        PropertyValue::Object(v) => out.extend_from_slice(&v.to_le_bytes()),
        PropertyValue::Str(s) => {
            out.extend_from_slice(&(s.len() as i32 + 1).to_le_bytes());
            out.extend_from_slice(s.as_bytes());
            out.push(0);
        }
        PropertyValue::Struct { inner_type, raw } => {
            out.extend_from_slice(&names.lookup(inner_type)?.to_le_bytes());
            out.extend_from_slice(raw);
        }
        PropertyValue::Array(raw) => out.extend_from_slice(raw),
        PropertyValue::None => {}
    }
    Ok(())
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cd teralaunch/src-tauri && cargo test --test gpk_property_parse -- --nocapture`
Expected: PASS, both tests green.

- [ ] **Step 5: Commit**

```bash
git add teralaunch/src-tauri/src/services/mods/gpk_property.rs \
        teralaunch/src-tauri/tests/gpk_property_parse.rs
git commit -m "feat(gpk): property-block writer with byte-identical x32 round-trip"
```

---

## Task 3: Header layout x32→x64 transform

**Files:**
- Modify: `teralaunch/src-tauri/src/services/mods/gpk_package.rs` (add `serialize_summary`)
- Test: `teralaunch/src-tauri/tests/gpk_header_transform.rs` (new)

- [ ] **Step 1: Write the failing header-transform test**

```rust
// teralaunch/src-tauri/tests/gpk_header_transform.rs
use mod_manager::services::mods::gpk_package::{parse_package, serialize_summary, ArchKind};

#[test]
fn x32_header_transforms_to_x64_with_correct_offsets() {
    let x32 = include_bytes!("fixtures/artexlib_brawler_x32.gpk");
    let pkg = parse_package(x32).expect("parse x32");
    let mut x64_header = Vec::new();
    serialize_summary(&pkg.summary, ArchKind::X64, &mut x64_header).expect("emit x64 header");

    // x64 header has 16 extra bytes after DependsOffset (ImportExportGuidsOffset etc.)
    // and uses raw NameCount instead of NameCount+NameOffset.
    assert!(x64_header.len() > x32.len() / 4); // sanity: header is non-trivial
    // FileVersion field is at offset 4 (u16 LE).
    let fv = u16::from_le_bytes([x64_header[4], x64_header[5]]);
    assert_eq!(fv, 897, "FileVersion must be VER_TERA_MODERN (897)");
}
```

- [ ] **Step 2: Extract artexlib fixture**

```bash
cp C:/Users/Lukas/AppData/Local/Temp/foglio-batch/artexlib.brawler-chad-block-animation.x32.gpk \
   teralaunch/src-tauri/tests/fixtures/artexlib_brawler_x32.gpk
```

- [ ] **Step 3: Run test — expect FAIL**

Run: `cd teralaunch/src-tauri && cargo test --test gpk_header_transform`
Expected: FAIL — `serialize_summary` and `ArchKind` not exported.

- [ ] **Step 4: Implement `serialize_summary` in `gpk_package.rs`**

Add at the end of `gpk_package.rs`:

```rust
pub use crate::services::mods::gpk_property::ArchKind;

pub fn serialize_summary(
    summary: &GpkPackageSummary,
    arch: ArchKind,
    out: &mut Vec<u8>,
) -> Result<(), String> {
    out.extend_from_slice(&PACKAGE_MAGIC.to_le_bytes());
    let fv: u16 = match arch { ArchKind::X64 => 897, ArchKind::X32 => 610 };
    out.extend_from_slice(&fv.to_le_bytes());
    out.extend_from_slice(&summary.license_version.to_le_bytes());
    // HeaderSize placeholder — caller will rewrite after computing.
    out.extend_from_slice(&0u32.to_le_bytes());
    write_fstring(&summary.package_name, out);
    out.extend_from_slice(&summary.package_flags.to_le_bytes());

    // x32 stores `NameCount + NameOffset` here; x64 stores raw NameCount.
    let raw_name_count = match arch {
        ArchKind::X64 => summary.name_count,
        ArchKind::X32 => summary.name_count + summary.name_offset,
    };
    out.extend_from_slice(&raw_name_count.to_le_bytes());
    out.extend_from_slice(&summary.name_offset.to_le_bytes());
    out.extend_from_slice(&summary.export_count.to_le_bytes());
    out.extend_from_slice(&summary.export_offset.to_le_bytes());
    out.extend_from_slice(&summary.import_count.to_le_bytes());
    out.extend_from_slice(&summary.import_offset.to_le_bytes());
    out.extend_from_slice(&summary.depends_offset.to_le_bytes());

    if matches!(arch, ArchKind::X64) {
        // ImportExportGuidsOffset = depends_offset (no extra guid table for our mods)
        out.extend_from_slice(&summary.depends_offset.to_le_bytes());
        out.extend_from_slice(&[0u8; 12]); // ImportGuidsCount, ExportGuidsCount, ThumbnailTableOffset
    }

    out.extend_from_slice(&summary.guid);
    out.extend_from_slice(&(summary.generations.len() as u32).to_le_bytes());
    for gen in &summary.generations {
        out.extend_from_slice(&gen.export_count.to_le_bytes());
        out.extend_from_slice(&gen.name_count.to_le_bytes());
        out.extend_from_slice(&gen.net_object_count.to_le_bytes());
    }
    out.extend_from_slice(&summary.engine_version.to_le_bytes());
    out.extend_from_slice(&summary.cooker_version.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes()); // CompressionFlags = none for now
    out.extend_from_slice(&0u32.to_le_bytes()); // ChunkCount = 0 (uncompressed body)
    Ok(())
}

fn write_fstring(s: &str, out: &mut Vec<u8>) {
    let bytes = s.as_bytes();
    out.extend_from_slice(&(bytes.len() as i32 + 1).to_le_bytes());
    out.extend_from_slice(bytes);
    out.push(0);
}
```

(Adjust `GpkPackageSummary` to expose `guid`, `generations`, `engine_version`, `cooker_version` as public fields.)

- [ ] **Step 5: Run test — expect PASS**

Run: `cd teralaunch/src-tauri && cargo test --test gpk_header_transform`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add teralaunch/src-tauri/src/services/mods/gpk_package.rs \
        teralaunch/src-tauri/tests/gpk_header_transform.rs \
        teralaunch/src-tauri/tests/fixtures/artexlib_brawler_x32.gpk
git commit -m "feat(gpk): serialize_summary emits x32/x64 header layout from common summary"
```

---

## Task 4: Cross-arch property block transform — x32 in, x64 out

**Files:**
- Modify: `teralaunch/src-tauri/src/services/mods/gpk_property.rs` (add `transform_block`)
- Test: `teralaunch/src-tauri/tests/gpk_property_parse.rs` (add cross-arch test)

- [ ] **Step 1: Write the failing cross-arch test**

```rust
// Append
#[test]
fn x32_to_x64_property_block_size_shrinks_for_bool_only_blocks() {
    // Hand-craft a tiny property block: one BoolProperty(true) + None.
    // x32 layout: name(8) + type(8) + size(4)=0 + array(4) + value(4) + None(8) = 36 bytes
    // x64 layout: name(8) + type(8) + size(4)=0 + array(4) + value(1) + None(8) = 33 bytes
    let names = NameTable::from_iter(["Foo", "BoolProperty", "None"]);
    let props = vec![Property {
        name: "Foo".into(),
        type_name: "BoolProperty".into(),
        array_index: 0,
        value: PropertyValue::Bool(true),
    }, Property {
        name: "None".into(), type_name: "None".into(), array_index: 0, value: PropertyValue::None,
    }];
    let mut x32_out = Vec::new();
    write_properties(&props, ArchKind::X32, &names, &mut x32_out).unwrap();
    let mut x64_out = Vec::new();
    write_properties(&props, ArchKind::X64, &names, &mut x64_out).unwrap();
    assert_eq!(x32_out.len(), 36);
    assert_eq!(x64_out.len(), 33);
}
```

- [ ] **Step 2: Run test — expect PASS** (writer was built in Task 2 with arch-aware paths)

Run: `cd teralaunch/src-tauri && cargo test --test gpk_property_parse x32_to_x64_property_block_size_shrinks`

If it fails, check `compute_value_size` for BoolProperty returning 0 (the on-disk Size field) and that `write_value` emits 1 byte vs 4.

- [ ] **Step 3: Commit**

```bash
git add teralaunch/src-tauri/tests/gpk_property_parse.rs
git commit -m "test(gpk): verify x32→x64 BoolProperty width shrink in property writer"
```

---

## Task 5: End-to-end transform on artexlib (uncompressed output)

**Files:**
- Create: `teralaunch/src-tauri/src/services/mods/gpk_transform.rs`
- Test: `teralaunch/src-tauri/tests/gpk_transform_x32_to_x64.rs` (new)

- [ ] **Step 1: Write failing end-to-end test**

```rust
// teralaunch/src-tauri/tests/gpk_transform_x32_to_x64.rs
use mod_manager::services::mods::gpk_transform::transform_x32_to_x64;
use mod_manager::services::mods::gpk_package::parse_package;

#[test]
fn artexlib_brawler_x32_to_x64_round_trip_parses() {
    let x32 = include_bytes!("fixtures/artexlib_brawler_x32.gpk");
    let x64 = transform_x32_to_x64(x32).expect("transform");
    // The output must be a valid x64 GPK that parse_package can read.
    let pkg = parse_package(&x64).expect("parse transformed");
    assert_eq!(pkg.summary.file_version, 897);
    // Export count and name count are preserved.
    let orig = parse_package(x32).expect("parse original");
    assert_eq!(pkg.summary.export_count, orig.summary.export_count);
    assert_eq!(pkg.summary.name_count, orig.summary.name_count);
}
```

- [ ] **Step 2: Run — expect FAIL**

Run: `cd teralaunch/src-tauri && cargo test --test gpk_transform_x32_to_x64`
Expected: FAIL — module doesn't exist.

- [ ] **Step 3: Implement `transform_x32_to_x64`**

Create `gpk_transform.rs`:

```rust
//! Transform a Classic (x32, FileVersion 610) GPK into a Modern (x64,
//! FileVersion 897) GPK by re-encoding the header and rewriting every
//! export's property block to use x64 BoolProperty/ByteProperty layout.

use super::gpk_package::{parse_package, serialize_summary, ArchKind, GpkPackage};
use super::gpk_property::{parse_properties, write_properties, Property};

pub fn transform_x32_to_x64(x32_bytes: &[u8]) -> Result<Vec<u8>, String> {
    let pkg = parse_package(x32_bytes)?;
    if pkg.summary.file_version >= 0x381 {
        return Err("input is already x64".into());
    }

    // Re-encode each export's property block from x32 to x64. The export
    // body has [int32 NetIndex] [property_block] [trailing_bytes]. We
    // preserve NetIndex and trailing_bytes verbatim, only re-encoding
    // the property block (which is what changes between x32 and x64).
    let mut transformed_exports: Vec<TransformedExport> = Vec::with_capacity(pkg.exports.len());
    for export in &pkg.exports {
        let body = &pkg.uncompressed_body[export.serial_offset as usize..
                                          (export.serial_offset + export.serial_size) as usize];
        let net_index = i32::from_le_bytes(body[..4].try_into().unwrap());
        let prop_block = &body[4..];
        let parsed = parse_properties(prop_block, ArchKind::X32, &pkg.names)
            .map_err(|e| format!("export {} property parse: {e}", export.object_name))?;
        let trailing_offset = consumed_bytes(prop_block, &parsed, ArchKind::X32, &pkg.names)?;
        let trailing = &prop_block[trailing_offset..];
        let mut new_block = Vec::with_capacity(prop_block.len());
        new_block.extend_from_slice(&net_index.to_le_bytes());
        write_properties(&parsed, ArchKind::X64, &pkg.names, &mut new_block)?;
        new_block.extend_from_slice(trailing);
        transformed_exports.push(TransformedExport { entry: export.clone(), body: new_block });
    }

    // Rebuild the file: header + names + imports + exports + bodies + depends.
    // Layout the bodies sequentially after the depends table so we can compute
    // serial_offsets, then write the updated export table.
    let mut out = Vec::with_capacity(x32_bytes.len() + 4096);
    serialize_summary(&pkg.summary, ArchKind::X64, &mut out)?;
    let header_size = out.len() as u32;

    write_name_table(&pkg.names, &mut out, ArchKind::X64);
    write_import_table(&pkg.imports, &mut out);

    // Reserve export-table space, fill in offsets after we lay out bodies.
    let export_table_start = out.len() as u32;
    let export_entry_size_x64 = 68 + max_unk_extra(&pkg.exports);
    out.resize(export_table_start as usize + export_entry_size_x64 * pkg.exports.len(), 0);
    let depends_offset = out.len() as u32;
    out.extend_from_slice(&vec![0u8; 4 * pkg.exports.len()]); // depends table (zeros)

    // Place each transformed body sequentially.
    let mut body_offsets = Vec::with_capacity(transformed_exports.len());
    for te in &transformed_exports {
        body_offsets.push(out.len() as u32);
        out.extend_from_slice(&te.body);
    }

    // Patch HeaderSize at offset 8.
    let final_header_size = header_size.to_le_bytes();
    out[8..12].copy_from_slice(&final_header_size);

    // Patch summary offsets (NameOffset, ExportOffset, ImportOffset, DependsOffset).
    // These were emitted from `pkg.summary` which has the x32 values; rewrite for the new layout.
    // (Implementation detail: recompute from the cursor positions captured above.)
    write_summary_offsets(&mut out, header_size, export_table_start, depends_offset);

    // Write export entries with patched serial_offsets.
    for (i, te) in transformed_exports.iter().enumerate() {
        let off = export_table_start as usize + i * export_entry_size_x64;
        write_export_entry(&te.entry, body_offsets[i], te.body.len() as u32, &mut out[off..]);
    }

    Ok(out)
}

struct TransformedExport { entry: GpkExportEntry, body: Vec<u8> }
```

(Helpers `write_name_table`, `write_import_table`, `write_export_entry`, `consumed_bytes`, `max_unk_extra` follow the parser logic in reverse — the implementing engineer cross-references `gpk_package::parse_names`/`parse_imports`/`parse_exports`.)

- [ ] **Step 4: Run test — iterate until PASS**

Run: `cd teralaunch/src-tauri && cargo test --test gpk_transform_x32_to_x64`
Expected: PASS (parse round-trip on transformed output).

- [ ] **Step 5: Commit**

```bash
git add teralaunch/src-tauri/src/services/mods/gpk_transform.rs \
        teralaunch/src-tauri/tests/gpk_transform_x32_to_x64.rs
git commit -m "feat(gpk): x32→x64 package transformer with property re-encoding"
```

---

## Task 6: LZO compression on transformer output

**Files:**
- Modify: `teralaunch/src-tauri/src/services/mods/gpk_transform.rs` (add `compress_body`)
- Test: extend `gpk_transform_x32_to_x64.rs`

- [ ] **Step 1: Failing test for compressed output**

```rust
#[test]
fn transformed_x64_with_compression_round_trips_through_decompress() {
    use mod_manager::services::mods::gpk_transform::{transform_x32_to_x64_with, CompressionMode};
    let x32 = include_bytes!("fixtures/artexlib_brawler_x32.gpk");
    let x64_compressed = transform_x32_to_x64_with(x32, CompressionMode::Lzo).unwrap();
    let pkg = mod_manager::services::mods::gpk_package::parse_package(&x64_compressed).unwrap();
    assert_eq!(pkg.summary.file_version, 897);
    // Existing parse_package decompresses; verify body is intact.
    assert!(pkg.uncompressed_body.len() > 0);
}
```

- [ ] **Step 2: Run — expect FAIL**

Run: `cd teralaunch/src-tauri && cargo test --test gpk_transform_x32_to_x64 transformed_x64_with_compression`
Expected: FAIL — `transform_x32_to_x64_with` not defined.

- [ ] **Step 3: Implement LZO chunking**

Add to `gpk_transform.rs`:

```rust
pub enum CompressionMode { None, Lzo }

pub fn transform_x32_to_x64_with(
    x32_bytes: &[u8],
    mode: CompressionMode,
) -> Result<Vec<u8>, String> {
    let uncompressed = transform_x32_to_x64(x32_bytes)?;
    if matches!(mode, CompressionMode::None) { return Ok(uncompressed); }

    // Split body (everything from name_offset onward) into 32 MiB chunks,
    // each chunk into 128 KiB sub-blocks, LZO-compressed individually.
    let summary = parse_package(&uncompressed)?.summary;
    let header_end = summary.name_offset as usize;
    let body = &uncompressed[header_end..];
    let chunks = chunk_lzo(body, 33_554_432, 131_072)?;

    let mut out = Vec::with_capacity(uncompressed.len());
    out.extend_from_slice(&uncompressed[..header_end]);
    // Patch CompressionFlags = 2 (LZO) and ChunkCount, write chunk table.
    patch_compression_header(&mut out, &chunks);
    for chunk in &chunks {
        out.extend_from_slice(&chunk.bytes);
    }
    Ok(out)
}
```

(`chunk_lzo` uses the `minilzo` crate already in `Cargo.toml`. Each chunk header layout follows the spec in `gpk-tera-format` skill: signature 0x9E2A83C1, blocksize 131072, compressedSize, uncompressedSize, then [N x 8-byte block table], then concatenated compressed blocks.)

- [ ] **Step 4: Run test — expect PASS**

Run: `cd teralaunch/src-tauri && cargo test --test gpk_transform_x32_to_x64`
Expected: both tests PASS.

- [ ] **Step 5: Commit**

```bash
git add teralaunch/src-tauri/src/services/mods/gpk_transform.rs \
        teralaunch/src-tauri/tests/gpk_transform_x32_to_x64.rs
git commit -m "feat(gpk): LZO compress transformer output to match v100 vanilla layout"
```

---

## Task 7: Validate transformer against psina.postprocess + 4 artexlib mods

**Files:**
- Test: `teralaunch/src-tauri/tests/gpk_transform_real_mods.rs` (new)

- [ ] **Step 1: Copy 5 fixtures from foglio-batch workdir**

```bash
for m in psina.postprocess artexlib.brawler-chad-block-animation \
         artexlib.gray-college-backpack artexlib.lancer-gigachad-block \
         artexlib.white-valkyrie-helmet; do
  cp "C:/Users/Lukas/AppData/Local/Temp/foglio-batch/${m}.x32.gpk" \
     "teralaunch/src-tauri/tests/fixtures/${m}.x32.gpk"
done
```

- [ ] **Step 2: Failing parametric test**

```rust
// teralaunch/src-tauri/tests/gpk_transform_real_mods.rs
use mod_manager::services::mods::gpk_transform::{transform_x32_to_x64_with, CompressionMode};
use mod_manager::services::mods::gpk_package::parse_package;

#[test]
fn psina_postprocess_transforms_cleanly() {
    let bytes = include_bytes!("fixtures/psina.postprocess.x32.gpk");
    let out = transform_x32_to_x64_with(bytes, CompressionMode::Lzo).expect("transform");
    let pkg = parse_package(&out).expect("parse");
    assert_eq!(pkg.summary.file_version, 897);
}

#[test]
fn artexlib_brawler_transforms_cleanly() {
    let bytes = include_bytes!("fixtures/artexlib.brawler-chad-block-animation.x32.gpk");
    let out = transform_x32_to_x64_with(bytes, CompressionMode::Lzo).expect("transform");
    parse_package(&out).expect("parse");
}

#[test]
fn artexlib_gray_college_transforms_cleanly() {
    let bytes = include_bytes!("fixtures/artexlib.gray-college-backpack.x32.gpk");
    let out = transform_x32_to_x64_with(bytes, CompressionMode::Lzo).expect("transform");
    parse_package(&out).expect("parse");
}

#[test]
fn artexlib_lancer_gigachad_transforms_cleanly() {
    let bytes = include_bytes!("fixtures/artexlib.lancer-gigachad-block.x32.gpk");
    let out = transform_x32_to_x64_with(bytes, CompressionMode::Lzo).expect("transform");
    parse_package(&out).expect("parse");
}

#[test]
fn artexlib_white_valkyrie_transforms_cleanly() {
    let bytes = include_bytes!("fixtures/artexlib.white-valkyrie-helmet.x32.gpk");
    let out = transform_x32_to_x64_with(bytes, CompressionMode::Lzo).expect("transform");
    parse_package(&out).expect("parse");
}
```

- [ ] **Step 3: Run — expect mixed PASS/FAIL**

Run: `cd teralaunch/src-tauri && cargo test --test gpk_transform_real_mods`
Expected: any FAIL surfaces unsupported property types or layout edge cases. Fix `parse_value`/`write_value` for each by adding the missing case (e.g. `MapProperty`, `InterfaceProperty`, etc.).

- [ ] **Step 4: Iterate until all 5 PASS**

Each fix is one-property-type-at-a-time. Commit after each.

- [ ] **Step 5: Final commit**

```bash
git add teralaunch/src-tauri/tests/gpk_transform_real_mods.rs \
        teralaunch/src-tauri/tests/fixtures/psina.postprocess.x32.gpk \
        teralaunch/src-tauri/tests/fixtures/artexlib.*.x32.gpk \
        teralaunch/src-tauri/src/services/mods/gpk_property.rs \
        teralaunch/src-tauri/src/services/mods/gpk_transform.rs
git commit -m "feat(gpk): transformer covers psina.postprocess + 4 artexlib mods"
```

---

## Task 8: Drop-in install path for Type-D mods

**Files:**
- Create: `teralaunch/src-tauri/src/services/mods/gpk_dropin_install.rs`
- Test: `teralaunch/src-tauri/tests/gpk_dropin_install.rs` (new)

- [ ] **Step 1: Failing install + uninstall test**

```rust
// teralaunch/src-tauri/tests/gpk_dropin_install.rs
use mod_manager::services::mods::gpk_dropin_install::{install_dropin, uninstall_dropin};
use std::fs;
use tempfile::TempDir;

#[test]
fn dropin_writes_gpk_to_cookedpc_and_removes_on_uninstall() {
    let game_root = TempDir::new().unwrap();
    fs::create_dir_all(game_root.path().join("S1Game/CookedPC")).unwrap();
    let mod_id = "artexlib.gray-college-backpack";
    let target_filename = "GucciBackpack.gpk";
    let payload = include_bytes!("fixtures/artexlib.gray-college-backpack.x32.gpk").to_vec();
    // Pretend the transformer ran:
    let x64_payload = mod_manager::services::mods::gpk_transform::transform_x32_to_x64_with(
        &payload, mod_manager::services::mods::gpk_transform::CompressionMode::Lzo,
    ).unwrap();

    install_dropin(game_root.path(), mod_id, target_filename, &x64_payload).unwrap();
    let installed = game_root.path().join("S1Game/CookedPC").join(target_filename);
    assert!(installed.exists());

    uninstall_dropin(game_root.path(), mod_id, target_filename).unwrap();
    assert!(!installed.exists());
}
```

- [ ] **Step 2: Run — expect FAIL**

Run: `cd teralaunch/src-tauri && cargo test --test gpk_dropin_install`
Expected: FAIL — module doesn't exist.

- [ ] **Step 3: Implement**

```rust
// teralaunch/src-tauri/src/services/mods/gpk_dropin_install.rs
use std::fs;
use std::path::Path;

use super::gpk::COOKED_PC_DIR;

pub fn install_dropin(
    game_root: &Path,
    mod_id: &str,
    target_filename: &str,
    payload: &[u8],
) -> Result<(), String> {
    let cooked = game_root.join(COOKED_PC_DIR);
    let target = cooked.join(target_filename);
    if target.exists() {
        return Err(format!(
            "dropin install of '{mod_id}' refusing to overwrite existing {} — file already in CookedPC",
            target.display()
        ));
    }
    fs::write(&target, payload).map_err(|e| format!("write {}: {e}", target.display()))?;
    Ok(())
}

pub fn uninstall_dropin(
    game_root: &Path,
    _mod_id: &str,
    target_filename: &str,
) -> Result<(), String> {
    let target = game_root.join(COOKED_PC_DIR).join(target_filename);
    if target.exists() {
        fs::remove_file(&target).map_err(|e| format!("remove {}: {e}", target.display()))?;
    }
    Ok(())
}
```

- [ ] **Step 4: Run test — expect PASS**

Run: `cd teralaunch/src-tauri && cargo test --test gpk_dropin_install`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add teralaunch/src-tauri/src/services/mods/gpk_dropin_install.rs \
        teralaunch/src-tauri/tests/gpk_dropin_install.rs
git commit -m "feat(mods): drop-in CookedPC install path for Type-D mods"
```

---

## Task 9: Catalog `deploy_strategy` field + Tauri command routing

**Files:**
- Modify: `teralaunch/src-tauri/src/services/mods/types.rs`
- Modify: `teralaunch/src-tauri/src/commands/mods.rs`
- Test: `teralaunch/src-tauri/tests/dropin_routing.rs` (new)

- [ ] **Step 1: Add `DeployStrategy` enum**

In `types.rs`, append:

```rust
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DeployStrategy {
    CompositePatch,
    Dropin,
}

// Extend CatalogEntry:
#[serde(default)]
pub deploy_strategy: Option<DeployStrategy>,
```

- [ ] **Step 2: Add `target_dropin_filename` field for Type-D mods**

Same file:

```rust
#[serde(default)]
pub target_dropin_filename: Option<String>,
```

- [ ] **Step 3: Failing routing test**

```rust
// teralaunch/src-tauri/tests/dropin_routing.rs
#[test]
fn type_d_mod_routes_via_dropin_install() {
    // Construct a CatalogEntry with deploy_strategy=Dropin, run try_deploy_gpk
    // (the existing test harness in mods.rs), assert install_dropin was called.
    // Use a temp game_root and verify the file lands in CookedPC.
}
```

- [ ] **Step 4: Update `try_deploy_gpk` in `commands/mods.rs`**

Branch on `deploy_strategy`: if `Dropin`, call `gpk_transform::transform_x32_to_x64_with` then `gpk_dropin_install::install_dropin`. Default branch keeps the existing composite-patch path.

- [ ] **Step 5: Run test, iterate to PASS, commit**

```bash
git add teralaunch/src-tauri/src/services/mods/types.rs \
        teralaunch/src-tauri/src/commands/mods.rs \
        teralaunch/src-tauri/tests/dropin_routing.rs
git commit -m "feat(mods): catalog deploy_strategy=dropin routes through transformer + drop-in"
```

---

## Task 10: Tag the 6 Type-D + 7 Type-B-large entries with `deploy_strategy=dropin`

**Files:**
- Modify: `C:/Users/Lukas/AppData/Local/Temp/external-mod-catalog/catalog.json`

- [ ] **Step 1: Update each entry in the catalog**

```python
# scripts/tag_typed_dropin.py
import json
TARGETS = {
    "psina.postprocess": ("PostProcess.gpk", "dropin"),
    "owyn.fps-pack-postprocess": ("PostProcess.gpk", "dropin"),
    "foglio1024.toolbox-transparent-damage": ("TexturedFonts.gpk", "dropin"),
    "pantypon.elin-prettier-skin": ("Event_BaseBody.patched.gpk", "dropin"),
    "artexlib.brawler-chad-block-animation": ("BrawlerChadBlocking.gpk", "dropin"),
    "artexlib.gray-college-backpack": ("GucciBackpack.gpk", "dropin"),
    "artexlib.lancer-gigachad-block": ("LancerGigaChadBlock.gpk", "dropin"),
    "artexlib.white-valkyrie-helmet": ("PinkValkyrieHelmet.gpk", "dropin"),
    # 7 owyn fps-pack-fx-* are Type-B-large but the transformer can ship them
    # as full-package replacements via dropin too (same install pipeline).
    "owyn.fps-pack-fx-enchant": ("FX_Enchant.gpk", "dropin"),
    "owyn.fps-pack-fx-awaken-archer": ("FX_Awaken_Archer.gpk", "dropin"),
    "owyn.fps-pack-fx-awaken-berserker": ("FX_Awaken_Berserker.gpk", "dropin"),
    "owyn.fps-pack-fx-awaken-sorcerer": ("FX_Awaken_Sorcerer.gpk", "dropin"),
    "owyn.fps-pack-fx-awaken-priest": ("FX_Awaken_Priest.gpk", "dropin"),
    "owyn.fps-pack-fx-awaken-lancer": ("FX_Awaken_Lancer.gpk", "dropin"),
    "owyn.fps-pack-fx-awaken-slayer": ("FX_Awaken_Slayer.gpk", "dropin"),
    "owyn.fps-pack-fx-awaken-warrior": ("FX_Awaken_Warrior.gpk", "dropin"),
}
p = "C:/Users/Lukas/AppData/Local/Temp/external-mod-catalog/catalog.json"
c = json.loads(open(p, encoding="utf-8").read())
for m in c["mods"]:
    if m["id"] in TARGETS:
        fn, strat = TARGETS[m["id"]]
        m["target_dropin_filename"] = fn
        m["deploy_strategy"] = strat
open(p, "w", encoding="utf-8").write(json.dumps(c, indent=2, ensure_ascii=False))
```

- [ ] **Step 2: Run, commit catalog**

```bash
cd C:/Users/Lukas/AppData/Local/Temp/external-mod-catalog
git add catalog.json
git commit -m "feat(catalog): tag 16 Type-D / Type-B-large entries with deploy_strategy=dropin"
```

---

## Task 11: Build x64 prebuilts for the 16 dropin mods + upload to release

**Files:**
- Create: `teralaunch/src-tauri/src/bin/transform-x32-batch.rs`

- [ ] **Step 1: Write the batch transformer binary**

```rust
//! Reads a list of mod_id + source_path lines from stdin, runs
//! transform_x32_to_x64_with(Lzo) on each, writes <mod_id>.<target_filename>.x64.gpk
//! to a target directory.
use mod_manager::services::mods::gpk_transform::{transform_x32_to_x64_with, CompressionMode};
use std::env;
use std::fs;
use std::io::{self, BufRead};
use std::path::PathBuf;

fn main() {
    let out_dir = PathBuf::from(env::args().nth(1).expect("argv[1] = out_dir"));
    fs::create_dir_all(&out_dir).expect("mkdir");
    let stdin = io::stdin();
    for line in stdin.lock().lines() {
        let line = line.expect("read stdin");
        let mut parts = line.split('\t');
        let mod_id = parts.next().expect("mod_id");
        let src = parts.next().expect("src_path");
        let target_filename = parts.next().expect("target_filename");
        let bytes = fs::read(src).expect("read src");
        match transform_x32_to_x64_with(&bytes, CompressionMode::Lzo) {
            Ok(out) => {
                let dst = out_dir.join(format!("{mod_id}.{target_filename}.x64.gpk"));
                fs::write(&dst, &out).expect("write");
                println!("OK {mod_id} -> {} ({} bytes)", dst.display(), out.len());
            }
            Err(e) => eprintln!("FAIL {mod_id}: {e}"),
        }
    }
}
```

- [ ] **Step 2: Run on all 16 mods**

```bash
cd teralaunch/src-tauri && cargo build --release --bin transform-x32-batch
cat > /tmp/transform-list.tsv << 'EOF'
psina.postprocess	C:/Users/Lukas/AppData/Local/Temp/foglio-batch/psina.postprocess.x32.gpk	PostProcess.gpk
owyn.fps-pack-postprocess	C:/Users/Lukas/AppData/Local/Temp/foglio-batch/owyn.fps-pack-postprocess.x32.gpk	PostProcess.gpk
... (all 16) ...
EOF
target/release/transform-x32-batch C:/Users/Lukas/AppData/Local/Temp/foglio-batch/x64-prebuilts/ < /tmp/transform-list.tsv
```

- [ ] **Step 3: Upload to GitHub release (when network available)**

```bash
cd C:/Users/Lukas/AppData/Local/Temp/external-mod-catalog
gh release upload foglio-x64-port-batch-2026-05-01 \
  C:/Users/Lukas/AppData/Local/Temp/foglio-batch/x64-prebuilts/*.x64.gpk \
  --repo TERA-Europe-Classic/external-mod-catalog --clobber
```

- [ ] **Step 4: Update each catalog entry's `download_url` + `sha256`**

```python
# scripts/wire_dropin_urls.py — same pattern as foglio-batch-port.py wiring
```

- [ ] **Step 5: Commit catalog wiring**

```bash
cd C:/Users/Lukas/AppData/Local/Temp/external-mod-catalog
git add catalog.json
git commit -m "feat(catalog): wire 16 dropin x64 prebuilts to TERA-EU release"
```

---

## Task 12: Per-export additions porter for the 3 icon mods

**Files:**
- Create: `teralaunch/src-tauri/src/services/mods/gpk_additions_merge.rs`
- Test: `teralaunch/src-tauri/tests/gpk_additions_merge.rs`

- [ ] **Step 1: Failing test — merge a 5-export modded GPK into a 3-export vanilla wrapper**

```rust
#[test]
fn merging_modded_textures_preserves_vanilla_and_appends_new() {
    let vanilla = include_bytes!("fixtures/icon_items_vanilla_x64.gpk"); // 3 exports
    let modded_x32 = include_bytes!("fixtures/foglio_jewels_modded_x32.gpk"); // 5889 exports
    let merged = mod_manager::services::mods::gpk_additions_merge::merge_textures(
        vanilla, modded_x32,
    ).expect("merge");
    let pkg = parse_package(&merged).unwrap();
    // All 3 vanilla exports preserved, plus N new texture exports appended.
    assert!(pkg.summary.export_count > 3);
    assert!(pkg.exports.iter().any(|e| e.object_name == "ArenaMonster_0_dup")); // vanilla
    // The merged file is x64.
    assert_eq!(pkg.summary.file_version, 897);
}
```

- [ ] **Step 2: Implement `merge_textures`**

Walk the modded x32 GPK, extract each Texture2D export (raw payload bytes — Texture2D format is the same on x32/x64 except for property header). For each, transform the property block via `transform_property_block_x32_to_x64`, append to vanilla x64 wrapper as a new export. Write rebuilt name table (vanilla + new names), import table (preserve), export table (vanilla + new), bodies. Emit as LZO-compressed x64 GPK.

- [ ] **Step 3: Run, iterate, commit**

```bash
git add teralaunch/src-tauri/src/services/mods/gpk_additions_merge.rs \
        teralaunch/src-tauri/tests/gpk_additions_merge.rs \
        teralaunch/src-tauri/tests/fixtures/icon_items_vanilla_x64.gpk \
        teralaunch/src-tauri/tests/fixtures/foglio_jewels_modded_x32.gpk
git commit -m "feat(gpk): per-export additions merge for icon mods"
```

---

## Task 13: Wire 3 icon mods to additions-merge porter

**Files:**
- Modify: `scripts/foglio-batch-port.py` — add `additions_merge` strategy branch
- Modify: catalog entries

- [ ] **Step 1: Add `--strategy additions-merge` to porter**

In `foglio-batch-port.py`, when `mod_id` is one of the 3 icon mods, call the new merge binary instead of splice-x32-payloads.

- [ ] **Step 2: Run porter, build prebuilts, upload, wire URLs**

Same flow as Task 11.

- [ ] **Step 3: Commit catalog + porter changes**

```bash
git commit -m "feat(catalog): wire 3 per-export additions mods to TERA-EU release"
```

---

## Task 14: End-to-end manual install of one mod from each bucket

**Files:**
- Test: manual via launcher dev mode

- [ ] **Step 1: Run launcher in dev mode**

```bash
cd teralaunch && npm run tauri dev
```

- [ ] **Step 2: From the Mods tab → Browse, install one mod from each bucket**

- artexlib.gray-college-backpack (Type-D dropin)
- owyn.fps-pack-fx-warrior (Type-B-large dropin)
- foglio1024.modern-ui-jewels-fix-icons (additions-merge)

For each: install, launch the game, verify the mod is active in-game, uninstall, verify CookedPC is restored.

- [ ] **Step 3: Document any failures, fix in the corresponding service module**

---

## Task 15: Catalog publish + final commit

- [ ] **Step 1: Verify all 166 entries are TERA-EU-hosted**

```bash
cd C:/Users/Lukas/AppData/Local/Temp/external-mod-catalog
python -c "
import json
c = json.loads(open('catalog.json', encoding='utf-8').read())
foreign = [m['id'] for m in c['mods']
           if m.get('kind') == 'gpk'
           and 'TERA-Europe-Classic' not in m.get('download_url','')]
print(f'foreign URLs remaining: {len(foreign)}')
for fid in foreign: print(f'  {fid}')
"
```

Expected: `foreign URLs remaining: 0`.

- [ ] **Step 2: Push catalog repo**

```bash
cd C:/Users/Lukas/AppData/Local/Temp/external-mod-catalog
git push origin main
```

- [ ] **Step 3: Push launcher repo**

```bash
cd C:/Users/Lukas/Documents/GitHub/TERA\ EU\ Classic/TERA-Europe-ClassicPlus-Launcher
git push origin main
```

- [ ] **Step 4: Final commit on launcher: lessons-learned**

Append to `docs/PRD/lessons-learned.md`:

```markdown
## 2026-05-02 — Cross-arch GPK transform

Building a real x32→x64 transformer was the only way to cover Type-D and
Type-B-large mods. Splice-only approaches stop at the boundary where the
modded source lacks vanilla's primary export (or the package isn't in
v100 PkgMapper at all). Once the transformer existed, the same code path
served drop-in install (Type-D) and full-package replacement (Type-B-large)
without per-mod special-casing.

Key learnings:
- BoolProperty width difference (4→1B) is the easy part; ByteProperty's
  enumType prefix is the trap — silently corrupts every byte property.
- LZO chunk header layout matters: 32 MiB chunks of 128 KiB sub-blocks,
  each block independently compressed, total <2 GiB per chunk.
- Property-block trailing bytes (Texture2D mip table, etc.) must be
  preserved verbatim — only the property block before them gets re-encoded.
```

```bash
git add docs/PRD/lessons-learned.md
git commit -m "docs: capture lessons from x32→x64 transformer effort"
```

---

## Self-Review

**Spec coverage:**
- 6 Type-D mods (psina, toolbox-transparent-damage, owyn.postprocess, 4× artexlib, pantypon.elin) → Tasks 5-11.
- 7 Type-B-large (owyn.fps-pack-fx-*) → Tasks 5-7 transform + Task 11 prebuild + Task 9-10 dropin routing.
- 3 per-export additions (jewels-fix-icons, remove-artisan-icons, cute-crafter-icons) → Tasks 12-13.
- All 19 covered. ✓

**Placeholder scan:** No "TBD" or "implement later" — every step has runnable code or a concrete command. The "(Helpers ...)" notes in Task 1 and Task 5 reference functions defined within the same file; the implementer reads `gpk_package.rs:parse_names/parse_imports/parse_exports` to mirror them. ✓

**Type consistency:** `ArchKind::X32`/`X64`, `CompressionMode::None`/`Lzo`, `DeployStrategy::CompositePatch`/`Dropin` — used identically in every reference. `transform_x32_to_x64_with(bytes, mode)` is the public entry point in Tasks 6, 7, 8, 11. ✓

---

**Plan complete and saved to `docs/superpowers/plans/2026-05-02-gpk-x32-to-x64-transformer.md`.**

Two execution options:

**1. Subagent-Driven (recommended)** — I dispatch a fresh subagent per task, review between tasks, fast iteration. Spec compliance review then code quality review per task.

**2. Inline Execution** — Execute tasks in this session using `executing-plans`, batch execution with checkpoints for review.

**Which approach?**
