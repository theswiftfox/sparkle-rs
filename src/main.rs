#![allow(unused_assignments)]
#![allow(dead_code)]
#[macro_use]
extern crate const_cstr;
extern crate nalgebra_glm as glm;

mod engine;
mod import;
mod input;
mod utils;
mod window;

use std::io;
use std::io::prelude::*;
fn pause() {
    let mut stdin = io::stdin();
    let mut stdout = io::stdout();

    // We want the cursor to stay at the end of the line, so we print without a newline and flush manually.
    write!(stdout, "Press enter to continue...").unwrap();
    stdout.flush().unwrap();

    // Read a single byte and discard
    let _ = stdin.read(&mut [0u8]).unwrap();
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    //pause();
    let mut renderer = engine::Renderer::create("Sparkle-rs");
    loop {
        if !renderer.update()? {
            break;
        }
    }
    renderer.cleanup();
    Ok(())
}

fn main() {
    match run() {
        Ok(_) => (),
        Err(e) => {
            println!("{}", e);
            pause();
        }
    }
}
