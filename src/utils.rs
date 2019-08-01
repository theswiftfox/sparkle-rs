/*
 *  Different Helper Functions 
 */

pub fn to_wide_str(value: &str) -> Vec<u16> {
    use std::os::windows::ffi::OsStrExt;
    std::ffi::OsStr::new(value).encode_wide().chain( std::iter::once(0)).collect()
}