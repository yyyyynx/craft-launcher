use std::{io, thread, time::Duration};

pub struct SingleInstance(isize);

impl SingleInstance {
    pub fn acquire() -> io::Result<Option<Self>> {
        // Local keeps separate Windows login sessions independent.
        let name: Vec<u16> = "Local\\CraftLauncher.SingleInstance\0".encode_utf16().collect();
        let handle = unsafe { CreateMutexW(std::ptr::null(), 0, name.as_ptr()) };
        let error = unsafe { GetLastError() };
        if handle == 0 {
            return Err(io::Error::from_raw_os_error(error as i32));
        }
        let guard = Self(handle);
        if error == 183 { // ERROR_ALREADY_EXISTS
            drop(guard);
            Ok(None)
        } else {
            Ok(Some(guard))
        }
    }
}

impl Drop for SingleInstance {
    fn drop(&mut self) {
        unsafe { CloseHandle(self.0); }
    }
}

pub fn restore_existing() {
    let title: Vec<u16> = "CraftLauncher\0".encode_utf16().collect();
    // A second launch can arrive before the first has created its window.
    for _ in 0..100 {
        let hwnd = unsafe { super::FindWindowW(std::ptr::null(), title.as_ptr()) };
        if hwnd != 0 {
            unsafe {
                ShowWindowAsync(hwnd, if IsIconic(hwnd) != 0 { 9 } else { 5 });
                SetForegroundWindow(hwnd);
            }
            return;
        }
        thread::sleep(Duration::from_millis(50));
    }
}

#[link(name = "kernel32")]
unsafe extern "system" {
    fn CreateMutexW(attributes: *const std::ffi::c_void, owner: i32, name: *const u16) -> isize;
    fn GetLastError() -> u32;
    fn CloseHandle(handle: isize) -> i32;
}

#[link(name = "user32")]
unsafe extern "system" {
    fn ShowWindowAsync(hwnd: isize, command: i32) -> i32;
    fn IsIconic(hwnd: isize) -> i32;
    fn SetForegroundWindow(hwnd: isize) -> i32;
}
