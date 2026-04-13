fn main() {
    tauri_build::build();

    #[cfg(feature = "sherpa")]
    {
        let sherpa_dir =
            std::env::var("SHERPA_ONNX_DIR").unwrap_or_else(|_| "vendor/sherpa-onnx".to_string());
        println!("cargo:rustc-link-search=native={}/lib", sherpa_dir);
        println!("cargo:rustc-link-lib=dylib=sherpa-onnx-c-api");
        println!("cargo:rustc-link-lib=dylib=onnxruntime");
    }
}
