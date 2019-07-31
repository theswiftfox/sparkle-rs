#![feature(fixed_size_array)]
mod d3d11;
mod window;

fn main() -> Result<(), &'static str> {
    println!("Hello, world!");

    let window = match window::Window::create_window(1280, 720, "main", "sparkle-rs") {
        Ok(window) => window,
        Err(e) => return Err(e)
    };
    let dx_context = match d3d11::D3D11Backend::init(&window) {
        Ok(ctx) => ctx,
        Err(e) => return Err(e)
    };
    loop {
        if !window.update() {
            break;
        }
    }

    Ok(())
}

