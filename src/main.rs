#![feature(fixed_size_array)]
#![feature(stmt_expr_attributes)]
#![feature(crate_visibility_modifier)]

#![allow(unused_assignments)]
#![allow(dead_code)]
#[macro_use]
extern crate const_cstr;
extern crate nalgebra_glm as glm;

mod import;
mod drawing;
mod input;
mod utils;
mod window;

use drawing::Renderer;

use std::io;
use std::io::prelude::*;
fn pause() {
    let mut stdin = io::stdin();
    let mut stdout = io::stdout();

    // We want the cursor to stay at the end of the line, so we print without a newline and flush manually.
    write!(stdout, "Press any key to continue...").unwrap();
    stdout.flush().unwrap();

    // Read a single byte and discard
    let _ = stdin.read(&mut [0u8]).unwrap();
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    pause();
    let mut renderer = drawing::create_backend(1280, 720, "Sparkle-rs");
    loop {
        if !renderer.update()? {
            break;
        }
    }
    renderer.cleanup();
    Ok(())
}
