#![windows_subsystem = "windows"]

#[link(name = "user32")]
extern "system" {
    fn MessageBoxW(hwnd: usize, text: *const u16, caption: *const u16, ty: u32) -> i32;
}

fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(Some(0)).collect()
}

fn main() {
    let mut args = std::env::args_os().skip(1);
    let caption = wide(&args.next().unwrap().to_string_lossy());
    let text = wide("ShibaPack automated test fixture");
    unsafe { MessageBoxW(0, text.as_ptr(), caption.as_ptr(), 0); }
}
