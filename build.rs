use std::process::Command;
// use std::env;
use std::error::Error;

fn main()-> Result<(), Box<dyn Error>> {
    //println!("cargo:rerun-if-changed=src/shaders");
    let _out_dir = "target/release/shaders";
    #[cfg(debug_assertions)]
    let _out_dir = "target/debug/shaders";
    // Create destination path if necessary
    std::fs::create_dir_all(_out_dir)?;

    for entry in std::fs::read_dir("src/shaders")? {
        let entry = entry?;

        if entry.file_type()?.is_file() {
            let p = entry.path();
            let name = p.file_stem().unwrap().to_string_lossy();

            let shader = match name.as_ref() {
                    "vertex" => Some("vs_5_0"),
                    "pixel" => Some("ps_5_0"),
                    _ => None
                };
            if shader != None {
                // compile shaders
                let cmd = Command::new("fxc").args(&["/T", &shader.unwrap(), "/Fo"])
                       .arg(&format!("{}/{}.cso", _out_dir, name))
                       .arg(p.to_str().unwrap())
                       .spawn().unwrap();
                let output = cmd.wait_with_output().unwrap();
                if !output.status.success() {
                    panic!(format!("Shader compile failed for: {}", p.file_name().unwrap().to_string_lossy()));
                }
            }
        }
    }

    Ok(())
}