#![feature(fixed_size_array)]
#![feature(stmt_expr_attributes)]
#![allow(unused_assignments)]
#![allow(dead_code)]
#[macro_use]
extern crate const_cstr;

mod drawing;
mod utils;
mod window;

use drawing::Renderer;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut renderer = drawing::create_backend(1280, 720, "Sparkle-rs");
    loop {
        if !renderer.update()? {
            break;
        }
    }
    renderer.cleanup();

    Ok(())
}
