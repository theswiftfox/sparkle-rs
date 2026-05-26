use std::{error::Error, ffi::CString};

use shader_slang::{self as slang, Downcast};

fn main() -> Result<(), Box<dyn Error>> {
    let global_session = slang::GlobalSession::new().unwrap();
    let target_desc = slang::TargetDesc::default()
        .format(slang::CompileTarget::Spirv)
        .profile(global_session.find_profile("glsl_450"));
    let search_paths = [
        CString::new("src/shaders/slang")?.into_raw() as *const i8,
        CString::new("src/shaders/slang/modules")?.into_raw() as *const i8,
    ];
    let session = global_session
        .create_session(
            &slang::SessionDesc::default()
                .targets(&[target_desc])
                .search_paths(&search_paths)
                .options(
                    &slang::CompilerOptions::default()
                        .optimization(slang::OptimizationLevel::High)
                        .vulkan_use_entry_point_name(true),
                ),
        )
        .unwrap();

    const SHADER_DIR: &str = "src/shaders/slang";
    let shaders = find_slang_files(SHADER_DIR);

    let spv_out = std::path::PathBuf::from("src/shaders/spv");
    std::fs::create_dir_all(&spv_out).unwrap();
    for shader in &shaders {
        let path_str = shader.display().to_string();

        let module = session.load_module(&path_str).unwrap();
        let Some(entry_point) = module.find_entry_point_by_name("main") else {
            println!(
                "Warning: No entry point named 'main' found in {}, skipping",
                shader.display()
            );
            continue;
        };

        let program = session
            .create_composite_component_type(&[
                module.downcast().clone(),
                entry_point.downcast().clone(),
            ])
            .unwrap();

        let linked_program = program.link().unwrap();
        let code = linked_program.entry_point_code(0, 0).unwrap();

        let mut out = spv_out.clone();
        out.push(
            shader
                .strip_prefix(SHADER_DIR)
                .unwrap()
                .with_extension("spv"),
        );
        std::fs::create_dir_all(out.parent().unwrap()).unwrap();
        std::fs::write(&out, code.as_slice()).unwrap();

        println!("Compiled: {} -> {}", shader.display(), out.display());
    }

    Ok(())
}

fn find_slang_files(dir: impl AsRef<std::path::Path>) -> Vec<std::path::PathBuf> {
    let mut files = Vec::new();
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                files.extend(find_slang_files(&path));
            } else if path.extension().is_some_and(|e| e == "slang") {
                files.push(path);
            }
        }
    }
    files
}
