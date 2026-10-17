//! Regression for the exact Tauri config union used by the packaged Release.
use tauri::utils::config::FrontendDist;

#[test]
fn windows_directory_string_is_an_external_url_not_embedded_assets() {
    let path = r"D:\a\_temp\日本語 space\frontend";
    let parsed: FrontendDist = serde_json::from_value(serde_json::json!(path)).unwrap();
    assert!(matches!(parsed, FrontendDist::Url(url) if url.scheme() == "d"));
}

#[test]
fn explicit_files_embed_assets_on_drive_paths_and_network_paths() {
    for directory in [
        r"D:\a\_temp\frontend",
        r"C:\日本語 space",
        r"\\server\share\frontend",
    ] {
        let paths = [
            format!("{directory}\\index.html"),
            format!("{directory}\\app.mjs"),
        ];
        let parsed: FrontendDist = serde_json::from_value(serde_json::json!(paths)).unwrap();
        let FrontendDist::Files(files) = parsed else {
            panic!("frontend assets were parsed as a URL/directory")
        };
        assert_eq!(files.len(), paths.len());
        for (file, expected) in files.iter().zip(paths) {
            assert_eq!(file, &std::path::PathBuf::from(expected));
        }
    }
}
