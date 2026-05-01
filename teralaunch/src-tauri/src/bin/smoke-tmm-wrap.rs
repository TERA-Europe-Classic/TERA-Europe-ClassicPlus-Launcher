#[allow(dead_code)] #[path = "../services/mods/gpk.rs"] mod gpk;
#[allow(dead_code)] #[path = "../services/mods/gpk_package.rs"] mod gpk_package;
#[allow(dead_code)] #[path = "../services/mods/mapper_extend.rs"] mod mapper_extend;
#[allow(dead_code)] #[path = "../services/mods/tmm_wrap.rs"] mod tmm_wrap;

use tmm_wrap::{wrap_as_tmm, TmmComposite, TmmModSpec};

const PACKAGE_MAGIC: u32 = 0x9E2A83C1;

fn synthesize_composite(object_path: &str) -> Vec<u8> {
    let mut buf = Vec::new();
    buf.extend_from_slice(&PACKAGE_MAGIC.to_le_bytes());
    buf.extend_from_slice(&897u16.to_le_bytes());
    buf.extend_from_slice(&14u16.to_le_bytes());
    buf.extend_from_slice(&0u32.to_le_bytes());
    let folder = format!("MOD:{object_path}");
    let folder_bytes = folder.as_bytes();
    let len = folder_bytes.len() as i32 + 1;
    buf.extend_from_slice(&len.to_le_bytes());
    buf.extend_from_slice(folder_bytes);
    buf.push(0);
    buf.extend_from_slice(&[0u8; 32]);
    buf
}

fn main() {
    // Single-composite round-trip
    let inner = synthesize_composite("S1UI_Test.TestObject_dup");
    let spec = TmmModSpec {
        container: "S1UI_Test_mod.gpk".to_string(),
        mod_name: "Test Mod".to_string(),
        mod_author: "Tester".to_string(),
        composites: vec![TmmComposite { bytes: inner.clone() }],
    };
    let wrapped = wrap_as_tmm(&spec).expect("wrap should succeed");
    let parsed = gpk::parse_mod_file(&wrapped).expect("parse should succeed");
    assert_eq!(parsed.container, "S1UI_Test_mod.gpk", "container mismatch: got '{}'", parsed.container);
    assert_eq!(parsed.mod_name, "Test Mod");
    assert_eq!(parsed.mod_author, "Tester");
    assert_eq!(parsed.packages.len(), 1);
    assert_eq!(parsed.packages[0].object_path, "S1UI_Test.TestObject_dup");
    assert!(parsed.packages[0].size > 0);
    println!("PASS single-composite round-trip");

    // Multi-composite
    let a = synthesize_composite("S1UI_A.A_dup");
    let b = synthesize_composite("S1UI_B.B_dup");
    let spec2 = TmmModSpec {
        container: "S1UI_Multi_mod.gpk".to_string(),
        mod_name: "Multi".to_string(),
        mod_author: "Tester".to_string(),
        composites: vec![TmmComposite { bytes: a.clone() }, TmmComposite { bytes: b.clone() }],
    };
    let wrapped2 = wrap_as_tmm(&spec2).expect("wrap2 should succeed");
    let parsed2 = gpk::parse_mod_file(&wrapped2).expect("parse2 should succeed");
    assert_eq!(parsed2.packages.len(), 2);
    assert_eq!(parsed2.packages[0].object_path, "S1UI_A.A_dup");
    assert_eq!(parsed2.packages[0].offset, 0);
    assert_eq!(parsed2.packages[1].object_path, "S1UI_B.B_dup");
    assert_eq!(parsed2.packages[1].offset, a.len() as i64);
    println!("PASS multi-composite round-trip");
}
