/*
 *  Different Helper Functions 
 */

pub fn to_wide_str(value: &str) -> Vec<u16> {
    use std::os::windows::ffi::OsStrExt;
    std::ffi::OsStr::new(value).encode_wide().chain( std::iter::once(0)).collect()
}

pub fn to_lpc_str(value: &str) -> &std::ffi::CStr {
    std::ffi::CStr::from_bytes_with_nul(value.as_bytes()).unwrap()
}