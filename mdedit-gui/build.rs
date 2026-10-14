fn main() {
    // 嵌入应用图标与 Windows 元信息（用 embed-resource，纯 Rust 无需 rc.exe）
    if std::path::Path::new("mdedit.ico").exists() {
        embed_resource::compile("app.rc", embed_resource::NONE);
    }
}
