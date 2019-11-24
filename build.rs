#![allow(unused_assignments)]

use std::error::Error;
use std::process::Command;

fn main() -> Result<(), Box<dyn Error>> {
    // copy over settings.ini
    let _out_dir_assets = "target/release/assets";
    #[cfg(debug_assertions)]
    let _out_dir_assets = "target/debug/assets";

    std::fs::create_dir_all(_out_dir_assets)?;
    match std::fs::copy(
        "assets/settings.ini",
        format!("{}/settings.ini", _out_dir_assets),
    ) {
        Ok(_) => (),
        Err(e) => println!("Error {} copying settings.ini", e),
    };

    //println!("cargo:rerun-if-changed=src/shaders");
    let _out_dir_shaders = "target/release/shaders";
    #[cfg(debug_assertions)]
    let _out_dir_shaders = "target/debug/shaders";
    // Create destination path if necessary
    std::fs::create_dir_all(_out_dir_shaders)?;

    for entry in std::fs::read_dir("src/shaders")? {
        let entry = entry?;
        
        if entry.file_type()?.is_file() {
            println!("File: {}", entry.path().display());
            let p = entry.path();
            let name = p.file_stem().unwrap().to_string_lossy();

            let shader = {
                let mut res = None;
                if name.contains("pixel") {
                    res = Some("ps_5_0");
                } else if name.contains("vertex") {
                    res = Some("vs_5_0");
                } else if name.contains("geom") {
                    res = Some("gs_5_0");
                }
                res
            };
            println!("ShaderType: {}", shader.unwrap_or("None"));
            if shader != None {
                if p.extension().unwrap() != "hlsl" {
                    println!("Skip.. {}", p.file_stem().unwrap().to_str().unwrap());
                    continue;
                }
                // compile shaders windows
                let cmd = Command::new("fxc")
                    .args(&["/T", &shader.unwrap(), "/Fo"])
                    .arg(&format!("{}/{}.cso", _out_dir_shaders, name))
                    .arg(p.to_str().unwrap())
                    .spawn()
                    .unwrap();
                let output = cmd.wait_with_output().unwrap();
                println!("{}", String::from_utf8(output.stdout)?);
                if !output.status.success() {
                    println!("{}",  String::from_utf8(output.stderr)?);
                    panic!(format!(
                        "Shader compile failed for: {}",
                        p.file_name().unwrap().to_string_lossy()
                    ));
                }
            }
            // } else {
            //     std::fs::copy(
            //         p.to_str().unwrap(),
            //         format!("{}/{}", _out_dir_shaders, p.file_name().unwrap().to_str().unwrap()),
            //     )?;
            // }
        }
    }
    Ok(())
}
