fn main() {
    tauri_build::build();

    #[cfg(target_os = "windows")]
    {
        // When running cargo test on Windows GNU, test executables placed in
        // target/debug/deps need WebView2Loader.dll in their directory.
        if let Ok(out_dir) = std::env::var("OUT_DIR") {
            let out_path = std::path::PathBuf::from(&out_dir);
            if let Some(target_dir) = out_path
                .ancestors()
                .find(|p| p.ends_with("target/debug") || p.ends_with("target\\debug"))
            {
                let src = target_dir.join("WebView2Loader.dll");
                let dst = target_dir.join("deps").join("WebView2Loader.dll");
                if src.exists() && dst.parent().map(|p| p.exists()).unwrap_or(false) {
                    let _ = std::fs::copy(&src, &dst);
                }
            }
        }
    }
}
