use std::error::Error;

fn main() -> Result<(), Box<dyn Error>> {
    // Copy settings.ini to the output directory
    #[cfg(not(debug_assertions))]
    let out_dir_assets = "target/release/assets";
    #[cfg(debug_assertions)]
    let out_dir_assets = "target/debug/assets";

    std::fs::create_dir_all(out_dir_assets)?;
    match std::fs::copy(
        "assets/settings.ini",
        format!("{}/settings.ini", out_dir_assets),
    ) {
        Ok(_) => (),
        Err(e) => println!("Error {} copying settings.ini", e),
    };

    Ok(())
}
