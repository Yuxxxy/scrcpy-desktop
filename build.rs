fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=assets/app.ico");

    #[cfg(windows)]
    {
        let mut resource = winresource::WindowsResource::new();
        // gpui 用 LoadImageW(模块, ID 1) 取窗口图标，因此图标必须占用 ID 1。
        resource
            .set_icon("assets/app.ico")
            .set("ProductName", "Scrcpy 桌面版")
            .set("FileDescription", "Scrcpy 桌面版")
            .set("LegalCopyright", "Apache-2.0");
        if let Err(error) = resource.compile() {
            println!("cargo:warning=嵌入应用图标失败：{error}");
        }
    }
}
