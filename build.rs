fn main() {
    #[cfg(target_os = "windows")]
    {
        println!("cargo:rerun-if-changed=assets/doze-icon.ico");
        let mut res = winresource::WindowsResource::new();
        res.set_icon("assets/doze-icon.ico");
        res.set("ProductName", "Doze");
        res.set(
            "FileDescription",
            "Doze — REST, WebSocket, and RPC (JSON-RPC / gRPC) Client",
        );
        res.compile()
            .expect("Failed compiling Windows icon resource for Doze");
    }
}
