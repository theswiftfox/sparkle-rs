#![feature(fixed_size_array)]
#![feature(stmt_expr_attributes)]
#![allow(unused_assignments)]
#[macro_use]
extern crate const_cstr;

mod d3d11;
mod utils;
mod window;

fn main() -> Result<(), &'static str> {
    let mut renderer = match d3d11::renderer::D3D11Renderer::create(1280, 720, "Sparkle-rs") {
        Ok(r) => r,
        Err(e) => return Err(e),
    };
    loop {
        if !renderer.update()? {
            break;
        }
    }
    renderer.cleanup();

    Ok(())
}
