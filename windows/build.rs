//! 將 `assets/cpime.ico` 應用程式圖示嵌入 `cpime-win.exe`（檔案總管、工作列與捷徑皆會顯示）

fn main() {
    println!("cargo:rerun-if-changed=cpime.rc");
    println!("cargo:rerun-if-changed=assets/cpime.ico");

    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        embed_resource::compile("cpime.rc", embed_resource::NONE)
            .manifest_optional()
            .expect("failed to embed CPIME-Next icon resource (cpime.rc)");
    }
}
