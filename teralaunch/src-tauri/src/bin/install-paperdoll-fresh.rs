// install-paperdoll-fresh — install a fresh-port paperdoll mod GPK into
// D:\Elinu via mapper redirection (TMM-style: drop the file in CookedPC,
// rewrite one CompositePackageMapper row to point at it).
//
// Inputs:
//   --game-root <path>    e.g. D:\Elinu
//   --mod-gpk <path>      the modded x64 GPK we just authored
//   --container-name <s>  Latin-only filename (no .gpk) the mod will live as
//                         in CookedPC. e.g. "RestylePaperdoll".
//   --object-path <s>     the composite object path the mod overrides, e.g.
//                         "c7a706fb_268926b3_1ddcb.PaperDoll_dup".
//
// What it does:
//   1. Verify D:\Elinu/S1Game/CookedPC/CompositePackageMapper.{dat,clean} exist
//      and (if both exist) match — if .clean is missing, back up first.
//   2. Read mod bytes; assert FileVersion=897, MOD: folder matches --object-path.
//   3. Decrypt the live mapper, find the row whose composite_name matches the
//      composite UID (the part of object-path before the dot), assert exists.
//   4. Rewrite the row: filename = container_name, offset = 0, size = mod_len.
//      Preserve composite_name + object_path (those are the keys the engine
//      uses).
//   5. Add the tmm_marker row if missing.
//   6. Re-serialize + encrypt → atomic write CompositePackageMapper.dat.
//   7. Copy the mod GPK into CookedPC/<container_name>.gpk (atomic).
//   8. Self-verify: re-decrypt mapper, confirm new row reads back correctly.
//
// What it does NOT do:
//   - Touch any composite container (.gpk in CookedPC). Vanilla containers stay
//     intact — that's the point of mapper redirection.
//   - Touch PkgMapper.dat (the logical→composite map; this row points the
//     composite UID, which PkgMapper independently maps from the logical name).

use std::env;
use std::fs;
use std::path::{Path, PathBuf};

#[path = "../services/mods/gpk.rs"]
mod gpk;

const USAGE: &str =
    "install-paperdoll-fresh --game-root <path> --mod-gpk <path> --container-name <name> --object-path <full_path>";

struct CliArgs {
    game_root: PathBuf,
    mod_gpk: PathBuf,
    container_name: String,
    object_path: String,
}

fn parse_args() -> Result<CliArgs, String> {
    let mut game_root: Option<PathBuf> = None;
    let mut mod_gpk: Option<PathBuf> = None;
    let mut container_name: Option<String> = None;
    let mut object_path: Option<String> = None;

    let mut iter = env::args().skip(1);
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--game-root" => game_root = iter.next().map(PathBuf::from),
            "--mod-gpk" => mod_gpk = iter.next().map(PathBuf::from),
            "--container-name" => container_name = iter.next(),
            "--object-path" => object_path = iter.next(),
            "-h" | "--help" => {
                println!("{USAGE}");
                std::process::exit(0);
            }
            other => return Err(format!("Unknown arg '{other}'")),
        }
    }

    Ok(CliArgs {
        game_root: game_root.ok_or("--game-root is required")?,
        mod_gpk: mod_gpk.ok_or("--mod-gpk is required")?,
        container_name: container_name.ok_or("--container-name is required")?,
        object_path: object_path.ok_or("--object-path is required")?,
    })
}

fn ensure_clean_backup(cooked_pc: &Path) -> Result<(), String> {
    let live = cooked_pc.join(gpk::MAPPER_FILE);
    let clean = cooked_pc.join(gpk::BACKUP_FILE);
    if !clean.exists() {
        if !live.exists() {
            return Err(format!(
                "Neither {} nor {} exists in {}",
                gpk::MAPPER_FILE,
                gpk::BACKUP_FILE,
                cooked_pc.display()
            ));
        }
        println!(
            "  No .clean backup yet — copying {} -> {} as the vanilla baseline",
            gpk::MAPPER_FILE,
            gpk::BACKUP_FILE
        );
        fs::copy(&live, &clean)
            .map_err(|e| format!("Failed to back up mapper: {e}"))?;
    }
    Ok(())
}

fn read_folder_name(bytes: &[u8]) -> Option<String> {
    // GPK layout: magic(4) + fileVersion(2) + licenseVersion(2) + headerSize(4)
    // + folder FString (i32 len + bytes).
    if bytes.len() < 16 {
        return None;
    }
    let len = i32::from_le_bytes([bytes[12], bytes[13], bytes[14], bytes[15]]);
    if len == 0 {
        return Some(String::new());
    }
    if len > 0 {
        let s = len as usize;
        let end = 16usize.checked_add(s)?;
        if end > bytes.len() {
            return None;
        }
        let null_trim = if s > 0 && bytes[16 + s - 1] == 0 { s - 1 } else { s };
        Some(String::from_utf8_lossy(&bytes[16..16 + null_trim]).to_string())
    } else {
        let count = (-len) as usize;
        let byte_len = count * 2;
        let end = 16usize.checked_add(byte_len)?;
        if end > bytes.len() {
            return None;
        }
        let mut u16_buf = Vec::with_capacity(count);
        for i in 0..count {
            u16_buf.push(u16::from_le_bytes([bytes[16 + i * 2], bytes[16 + i * 2 + 1]]));
        }
        let s = String::from_utf16_lossy(&u16_buf);
        Some(s.trim_end_matches('\0').to_string())
    }
}

fn main() {
    if let Err(e) = run() {
        eprintln!("FAIL: {e}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let args = parse_args()?;
    println!("== install-paperdoll-fresh ==");
    println!("game_root:      {}", args.game_root.display());
    println!("mod_gpk:        {}", args.mod_gpk.display());
    println!("container_name: {}", args.container_name);
    println!("object_path:    {}", args.object_path);
    println!();

    let cooked_pc = args.game_root.join(gpk::COOKED_PC_DIR);
    if !cooked_pc.is_dir() {
        return Err(format!("CookedPC dir does not exist: {}", cooked_pc.display()));
    }

    if !gpk::is_safe_gpk_container_filename(&format!("{}.gpk", args.container_name)) {
        return Err(format!(
            "Container name '{}' is unsafe (contains separators or escapes)",
            args.container_name
        ));
    }

    // The composite UID is the part of object_path before the first dot.
    let composite_uid = args
        .object_path
        .split_once('.')
        .map(|(a, _)| a)
        .ok_or_else(|| format!("object_path '{}' has no '.'", args.object_path))?
        .to_string();
    println!("composite_uid:  {composite_uid}");

    // Read + verify mod GPK.
    let mod_bytes =
        fs::read(&args.mod_gpk).map_err(|e| format!("Read mod GPK failed: {e}"))?;
    if mod_bytes.len() < 16 {
        return Err("Mod GPK too small".into());
    }
    let magic = u32::from_le_bytes([mod_bytes[0], mod_bytes[1], mod_bytes[2], mod_bytes[3]]);
    if magic != 0x9E2A83C1 {
        return Err(format!("Mod GPK magic 0x{magic:08X} != 0x9E2A83C1"));
    }
    let file_version = u16::from_le_bytes([mod_bytes[4], mod_bytes[5]]);
    if file_version != 897 {
        return Err(format!("Mod GPK FileVersion {file_version} != 897 (must be x64)"));
    }
    let folder_name = read_folder_name(&mod_bytes).unwrap_or_default();
    let expected_folder = format!("MOD:{}", args.object_path);
    if folder_name.trim_end_matches('\0') != expected_folder {
        return Err(format!(
            "Mod GPK folder '{folder_name}' != expected '{expected_folder}'"
        ));
    }
    println!("mod GPK ok: {} bytes, file_version=897, folder='{folder_name}'", mod_bytes.len());

    // Backup baseline if needed.
    ensure_clean_backup(&cooked_pc)?;

    // Decrypt + parse current mapper.
    let live_path = cooked_pc.join(gpk::MAPPER_FILE);
    let live_enc = fs::read(&live_path)
        .map_err(|e| format!("Read live mapper failed: {e}"))?;
    let live_dec = gpk::decrypt_mapper(&live_enc);
    let live_text = String::from_utf8_lossy(&live_dec).to_string();
    let mut map = gpk::parse_mapper(&live_text);

    // Find the row by composite_uid.
    let entry_owned = map
        .get(&composite_uid)
        .cloned()
        .ok_or_else(|| format!("Composite UID '{composite_uid}' not in current mapper"))?;
    println!(
        "  current entry: filename={} object_path={} offset={} size={}",
        entry_owned.filename, entry_owned.object_path, entry_owned.offset, entry_owned.size
    );

    // Rewrite the row.
    let new_filename = args.container_name.clone();
    let new_size = mod_bytes.len() as i64;
    let mut new_entry = entry_owned.clone();
    new_entry.filename = new_filename.clone();
    new_entry.offset = 0;
    new_entry.size = new_size;
    map.insert(composite_uid.clone(), new_entry);
    println!(
        "  new entry:     filename={} object_path={} offset=0 size={}",
        new_filename, args.object_path, new_size
    );

    // Add the tmm_marker row if missing — TMM uses this to detect "we have
    // touched this dat" and to trigger re-backup logic when TERA repair wipes
    // it.
    if !map.contains_key("tmm_marker") {
        map.insert(
            "tmm_marker".to_string(),
            gpk::MapperEntry {
                filename: "tmm_marker".to_string(),
                composite_name: "tmm_marker".to_string(),
                object_path: "tmm_marker".to_string(),
                offset: 0,
                size: 0,
            },
        );
        println!("  added tmm_marker row");
    }

    // Re-serialize + encrypt + atomic write.
    let new_text = gpk::serialize_mapper(&map);
    let new_enc = gpk::encrypt_mapper(new_text.as_bytes());
    gpk::write_atomic_file(&live_path, &new_enc)
        .map_err(|e| format!("Atomic write mapper failed: {e}"))?;
    println!("  wrote {} encrypted bytes to {}", new_enc.len(), live_path.display());

    // Copy mod GPK to CookedPC.
    let dest = cooked_pc.join(format!("{}.gpk", args.container_name));
    gpk::copy_atomic(&args.mod_gpk, &dest)
        .map_err(|e| format!("Atomic copy mod GPK failed: {e}"))?;
    println!("  copied mod GPK to {}", dest.display());

    // Self-verify: re-read and parse mapper, confirm row.
    let verify_enc = fs::read(&live_path)
        .map_err(|e| format!("Verify read mapper failed: {e}"))?;
    let verify_dec = gpk::decrypt_mapper(&verify_enc);
    let verify_text = String::from_utf8_lossy(&verify_dec).to_string();
    let verify_map = gpk::parse_mapper(&verify_text);
    let verify_entry = verify_map
        .get(&composite_uid)
        .ok_or_else(|| format!("Verify: composite_uid '{composite_uid}' missing after write"))?;
    if verify_entry.filename != new_filename
        || verify_entry.offset != 0
        || verify_entry.size != new_size
    {
        return Err(format!(
            "Verify: row mismatch (got filename={} offset={} size={}, want filename={} offset=0 size={})",
            verify_entry.filename, verify_entry.offset, verify_entry.size, new_filename, new_size
        ));
    }
    println!("\nself-verify: PASS");
    println!(
        "  row reads back as: filename={} composite_name={} object_path={} offset={} size={}",
        verify_entry.filename,
        verify_entry.composite_name,
        verify_entry.object_path,
        verify_entry.offset,
        verify_entry.size
    );
    println!("\nDONE — install successful. Launch the game and open the equipment window.");
    Ok(())
}
