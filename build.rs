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

    let mut release = false;
    #[cfg(debug_assertions)]
    {
        release = false;
    }

    for entry in std::fs::read_dir("src/shaders")? {
        let entry = entry?;

        if entry.file_type()?.is_file() {
            let p = entry.path();
            let name = p.file_stem().unwrap().to_string_lossy();

            if release {
                let shader = match name.as_ref() {
                    "vertex" => Some("vs_5_0"),
                    "pixel" => Some("ps_5_0"),
                    _ => None,
                };
                if shader != None {
                    #[cfg(target_os = "windows")]
                    {
                        if p.file_stem().unwrap() != "hlsl" {
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
                        if !output.status.success() {
                            panic!(format!(
                                "Shader compile failed for: {}",
                                p.file_name().unwrap().to_string_lossy()
                            ));
                        }
                    }
                    #[cfg(target_os = "linux")]
                    {
                        // compile shaders linux
                    }
                }
            } else {
                std::fs::copy(
                    p.to_str().unwrap(),
                    format!("{}/{}", _out_dir_shaders, p.file_name().unwrap().to_str().unwrap()),
                )?;
            }
        }
    }
    Ok(())
}
