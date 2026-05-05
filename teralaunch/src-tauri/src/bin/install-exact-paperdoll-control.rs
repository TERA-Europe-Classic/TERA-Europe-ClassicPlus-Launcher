use std::{
    env, fs,
    path::{Path, PathBuf},
};

#[path = "../services/mods/gpk.rs"]
mod gpk;

const CONTAINER_STEM: &str = "TMMExactPaperDoll";
const CONTAINER_FILENAME: &str = "TMMExactPaperDoll.gpk";

fn main() {
    if let Err(err) = run() {
        eprintln!("FAIL: {err}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let args = Args::parse()?;
    let built = build_exact_container(&args.slices_dir)?;
    install_exact_container(&args.game_root, &built)?;
    println!(
        "installed exact-byte container={CONTAINER_FILENAME} mapper-file={CONTAINER_STEM} packages={}",
        built.packages.len()
    );
    for package in &built.packages {
        println!(
            "package={} offset={} size={}",
            package.object_path, package.offset, package.size
        );
    }
    Ok(())
}

struct Args {
    game_root: PathBuf,
    slices_dir: PathBuf,
}

impl Args {
    fn parse() -> Result<Self, String> {
        let mut args = env::args().skip(1);
        let game_root = PathBuf::from(args.next().ok_or_else(usage)?);
        let slices_dir = PathBuf::from(args.next().ok_or_else(usage)?);
        if args.next().is_some() {
            return Err(usage());
        }
        Ok(Self {
            game_root,
            slices_dir,
        })
    }
}

struct BuiltContainer {
    bytes: Vec<u8>,
    packages: Vec<gpk::ModPackage>,
}

fn build_exact_container(slices_dir: &Path) -> Result<BuiltContainer, String> {
    let mut bytes = Vec::new();
    let mut packages = Vec::new();

    for (filename, object_path) in paperdoll_targets() {
        let slice_path = slices_dir.join(filename);
        let slice = fs::read(&slice_path)
            .map_err(|e| format!("failed to read '{}': {e}", slice_path.display()))?;
        if slice.get(0..4) != Some(&0x9E2A83C1u32.to_le_bytes()) {
            return Err(format!("'{}' is not a GPK slice", slice_path.display()));
        }
        let offset = bytes.len() as i64;
        let size = slice.len() as i64;
        bytes.extend_from_slice(&slice);
        packages.push(gpk::ModPackage {
            object_path: object_path.to_string(),
            offset,
            size,
            ..Default::default()
        });
    }

    Ok(BuiltContainer { bytes, packages })
}

fn install_exact_container(game_root: &Path, built: &BuiltContainer) -> Result<(), String> {
    let cooked_pc = game_root.join(gpk::COOKED_PC_DIR);
    fs::create_dir_all(&cooked_pc)
        .map_err(|e| format!("failed to create '{}': {e}", cooked_pc.display()))?;

    let container_path = cooked_pc.join(CONTAINER_FILENAME);
    gpk::write_atomic_file(&container_path, &built.bytes)
        .map_err(|e| format!("failed to write '{}': {e}", container_path.display()))?;

    let mapper_path = cooked_pc.join(gpk::MAPPER_FILE);
    let encrypted = fs::read(&mapper_path)
        .map_err(|e| format!("failed to read '{}': {e}", mapper_path.display()))?;
    let plain = String::from_utf8_lossy(&gpk::decrypt_mapper(&encrypted)).to_string();
    let mut mapper = gpk::parse_mapper_strict(&plain)?;

    let modfile = gpk::ModFile {
        container: CONTAINER_STEM.to_string(),
        region_lock: true,
        packages: built.packages.clone(),
        ..Default::default()
    };
    gpk::apply_mod_patches(&mut mapper, &modfile)?;

    let encrypted = gpk::encrypt_mapper(gpk::serialize_mapper(&mapper).as_bytes());
    gpk::write_atomic_file(&mapper_path, &encrypted)
        .map_err(|e| format!("failed to write '{}': {e}", mapper_path.display()))
}

fn paperdoll_targets() -> [(&'static str, &'static str); 11] {
    [
        (
            "PaperDoll_0_0_dup.gpk",
            "ffe86d35_e425ee9e_33ba.PaperDoll_0_0_dup",
        ),
        (
            "PaperDoll_0_1_dup.gpk",
            "ffe86d35_4758f2f8_33b9.PaperDoll_0_1_dup",
        ),
        (
            "PaperDoll_1_0_dup.gpk",
            "ffe86d35_9a62ff60_33b8.PaperDoll_1_0_dup",
        ),
        (
            "PaperDoll_1_1_dup.gpk",
            "ffe86d35_391fe306_33b7.PaperDoll_1_1_dup",
        ),
        (
            "PaperDoll_2_0_dup.gpk",
            "ffe86d35_7136aa7e_33b6.PaperDoll_2_0_dup",
        ),
        (
            "PaperDoll_2_1_dup.gpk",
            "ffe86d35_d24bb618_33b5.PaperDoll_2_1_dup",
        ),
        (
            "PaperDoll_3_0_dup.gpk",
            "ffe86d35_f71bb80_33b4.PaperDoll_3_0_dup",
        ),
        (
            "PaperDoll_3_1_dup.gpk",
            "ffe86d35_ac0ca7e6_33b3.PaperDoll_3_1_dup",
        ),
        (
            "PaperDoll_4_0_dup.gpk",
            "ffe86d35_1a010c57_33b2.PaperDoll_4_0_dup",
        ),
        (
            "PaperDoll_4_1_dup.gpk",
            "ffe86d35_b97c1031_33b1.PaperDoll_4_1_dup",
        ),
        (
            "PaperDoll_5_1_dup.gpk",
            "ffe86d35_c73b01cf_33b0.PaperDoll_5_1_dup",
        ),
    ]
}

fn usage() -> String {
    "usage: install-exact-paperdoll-control <game-root> <resource-targets-dir>".to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_container_offsets_are_cumulative() {
        let first = vec![0xC1, 0x83, 0x2A, 0x9E, 1];
        let second = vec![0xC1, 0x83, 0x2A, 0x9E, 2, 2];
        let dir = tempfile::tempdir().expect("tempdir");
        for (idx, (filename, _)) in paperdoll_targets().iter().enumerate() {
            let data = if idx == 0 { &first } else { &second };
            fs::write(dir.path().join(filename), data).expect("write fixture");
        }

        let built = build_exact_container(dir.path()).expect("build exact container");

        assert_eq!(built.packages[0].offset, 0);
        assert_eq!(built.packages[0].size, first.len() as i64);
        assert_eq!(built.packages[1].offset, first.len() as i64);
        assert_eq!(built.packages[1].size, second.len() as i64);
    }

    #[test]
    fn exact_container_ranges_preserve_slice_bytes() {
        let first = vec![0xC1, 0x83, 0x2A, 0x9E, 1, 3, 5];
        let second = vec![0xC1, 0x83, 0x2A, 0x9E, 2, 4, 6, 8];
        let dir = tempfile::tempdir().expect("tempdir");
        for (idx, (filename, _)) in paperdoll_targets().iter().enumerate() {
            let data = if idx == 0 { &first } else { &second };
            fs::write(dir.path().join(filename), data).expect("write fixture");
        }

        let built = build_exact_container(dir.path()).expect("build exact container");
        let first_range = package_range(&built.packages[0]);
        let second_range = package_range(&built.packages[1]);

        assert_eq!(&built.bytes[first_range], first.as_slice());
        assert_eq!(&built.bytes[second_range], second.as_slice());
    }

    #[test]
    fn exact_control_uses_extensionless_mapper_filename() {
        let modfile = gpk::ModFile {
            container: CONTAINER_STEM.to_string(),
            ..Default::default()
        };

        assert_eq!(modfile.container, "TMMExactPaperDoll");
        assert_eq!(CONTAINER_FILENAME, "TMMExactPaperDoll.gpk");
    }

    fn package_range(package: &gpk::ModPackage) -> std::ops::Range<usize> {
        let start = package.offset as usize;
        start..start + package.size as usize
    }
}
