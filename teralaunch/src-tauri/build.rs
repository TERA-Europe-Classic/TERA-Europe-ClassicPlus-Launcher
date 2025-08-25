fn main() {
    #[cfg(target_os = "windows")]
    {
        // Let tauri-build embed our custom manifest to request admin.
        // This integrates with tauri's resource pipeline and avoids duplicates.
        let mut windows = tauri_build::WindowsAttributes::new();
        windows = windows.app_manifest(include_str!("windows-app-manifest.xml"));
        let attrs = tauri_build::Attributes::new().windows_attributes(windows);
        tauri_build::try_build(attrs).expect("Failed to build with custom Windows manifest");
        return;
    }

    #[cfg(not(target_os = "windows"))]
    tauri_build::build();
}
