use std::error::Error;

fn main() -> Result<(), Box<dyn Error>> {
    // Copy settings.ini to the output directory
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

    Ok(())
}
