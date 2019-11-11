use ini::Ini;

const SETTINGS_FILE: &str = "assets/settings.ini";

pub struct Settings {
    // Rendering
    pub resolution: (u32, u32),
    pub fullscreen: bool,

    pub level: Option<String>,

    // camera
    pub camera_fov: f32,
    pub view_distance: f32,

    // engine
    pub dx_validation: bool,
}

impl Settings {
    pub fn load() -> Settings {
        let mut settings = Settings::default();
        let mut settings_file = std::env::current_exe()
            .unwrap()
            .parent()
            .unwrap()
            .to_path_buf();
        settings_file.push(SETTINGS_FILE);
        let ini = match Ini::load_from_file(settings_file) {
            Ok(ini) => ini,
            Err(_) => return settings,
        };
        if let Some(display) = ini.section(Some("Display")) {
            match (display.get("Width"), display.get("Height")) {
                (Some(w), Some(h)) => match (w.parse::<u32>(), h.parse::<u32>()) {
                    (Ok(w), Ok(h)) => settings.resolution = (w, h),
                    _ => (),
                },
                _ => (),
            };
        }
        if let Some(camera) = ini.section(Some("Camera")) {
            match camera.get("FOV") {
                Some(raw) => match raw.parse::<f32>() {
                    Ok(fov) => settings.camera_fov = fov,
                    _ => (),
                },
                _ => (),
            };
            match camera.get("RenderDistance") {
                Some(raw) => match raw.parse::<f32>() {
                    Ok(rd) => settings.view_distance = rd,
                    _ => (),
                },
                _ => (),
            };
        }
        if let Some(game_settings) = ini.section(Some("Game")) {
            match game_settings.get("Level") {
                Some(l) => settings.level = Some(l.to_string()),
                _ => (),
            };
        }
        if let Some(engine_settings) = ini.section(Some("Engine")) {
            match engine_settings.get("Validation") {
                Some(v) => match v.parse::<bool>() {
                    Ok(b) => settings.dx_validation = b,
                    Err(_) => match v.parse::<u32>() {
                        Ok(i) => settings.dx_validation = i == 1,
                        _ => (),
                    },
                },
                _ => (),
            };
        }

        settings
    }
}

impl std::default::Default for Settings {
    fn default() -> Settings {
        Settings {
            resolution: (1024, 768),
            fullscreen: false,
            level: None,
            camera_fov: 70.0,
            view_distance: 1000.0,
            dx_validation: false,
        }
    }
}
