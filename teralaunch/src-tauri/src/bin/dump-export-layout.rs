#[path = "../services/mods/gpk_package.rs"] mod gpk_package;
use std::env;
use std::fs;
fn main() {
    let path = env::args().nth(1).expect("usage: dump-export-layout <path>");
    let raw = fs::read(&path).unwrap();
    let bytes = gpk_package::extract_uncompressed_package_bytes(&raw).unwrap();
    let pkg = gpk_package::parse_package(&bytes).unwrap();
    println!("file_size={}", bytes.len());
    println!("name_offset={}", pkg.summary.name_offset);
    println!("import_offset={}", pkg.summary.import_offset);
    println!("export_offset={}", pkg.summary.export_offset);
    println!("depends_offset={}", pkg.summary.depends_offset);
    let mut max_end: usize = 0;
    let mut min_start: usize = usize::MAX;
    for e in &pkg.exports {
        if let Some(off) = e.serial_offset {
            let end = off as usize + e.serial_size as usize;
            if end > max_end { max_end = end; }
            if (off as usize) < min_start { min_start = off as usize; }
            println!("  {} class={:?} serial_offset={} size={} end={}",
                e.object_path, e.class_name, off, e.serial_size, end);
        }
    }
    println!("first_payload_offset={}", min_start);
    println!("payload_region_end={}", max_end);
    let export_table_end = pkg.summary.export_offset as usize + pkg.exports.len() * 68;
    println!("export_table_end (approx)={}", export_table_end);
    println!();
    println!("Applier check: payload_region_end ({}) <= export_offset ({})? {}",
        max_end, pkg.summary.export_offset, max_end <= pkg.summary.export_offset as usize);
    println!("Applier check: export_table_end ({}) > first_payload ({})? {}",
        export_table_end, min_start, export_table_end > min_start);
}
