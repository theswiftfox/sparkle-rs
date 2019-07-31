#![feature(fixed_size_array)]
mod d3d11;
mod window;

fn main() -> Result<(), &'static str> {
    println!("Hello, world!");

    let window = match window::window::Window::create_window(1280, 720, "main", "sparkle-rs") {
        Ok(window) => window,
        Err(e) => return Err(e),
    };

    loop {
        if !window.update() {
            break;
        }
    }

    Ok(())
}

