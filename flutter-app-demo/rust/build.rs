fn main() {
    #[cfg(target_os="windows")] {
        let mut res = winres::WindowsResource::new();
        res.set_icon_with_id("./assets/icon.ico", "GLFW_ICON");
        res.compile().unwrap();
    }
}
