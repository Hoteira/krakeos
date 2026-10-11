// Settings: optional TOML files in /config. A missing file, a missing key or
// a typo falls back to the defaults, so an app always starts.

use std::fs::{self, File};
use std::io::{Read, Write};

use serde::de::DeserializeOwned;
use serde::Deserialize;

pub const DIR: &str = "/config";

pub fn path(name: &str) -> String {
    format!("{}/{}.toml", DIR, name)
}

// Read and parse /config/<name>.toml. Errors are reported on stderr and
// give the defaults.
pub fn load<T: DeserializeOwned + Default>(name: &str) -> T {
    let mut text = String::new();
    match File::open(path(name)) {
        Ok(mut f) => {
            let _ = f.read_to_string(&mut text);
        }
        Err(_) => return T::default(),
    }
    match toml::from_str(&text) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("config: {}: {}", path(name), e);
            T::default()
        }
    }
}

// Like load, but returns the parse error so an app can show it.
pub fn check<T: DeserializeOwned>(name: &str) -> Result<(), String> {
    let text = fs::read_to_string(path(name)).map_err(|_| "cannot read the file".to_string())?;
    toml::from_str::<T>(&text).map(|_| ()).map_err(|e| e.message().to_string())
}

// Make sure /config/<name>.toml exists, writing default_text if not.
// Returns its path.
pub fn ensure(name: &str, default_text: &str) -> String {
    let p = path(name);
    if File::open(&p).is_err() {
        let _ = fs::create_dir(DIR);
        if let Ok(mut f) = File::create(&p) {
            let _ = f.write_all(default_text.as_bytes());
        }
    }
    p
}

// system.toml

#[derive(Deserialize, Clone, PartialEq, Debug)]
#[serde(default)]
pub struct System {
    pub keyboard: Keyboard,
    pub display: Display,
}

#[derive(Deserialize, Clone, PartialEq, Debug)]
#[serde(default)]
pub struct Keyboard {
    // "it" or "us".
    pub layout: String,
}

#[derive(Deserialize, Clone, PartialEq, Debug)]
#[serde(default)]
pub struct Display {
    pub wallpaper: String,
    // Text size in pixels; 0 picks one from the screen height.
    pub font_size: f32,
    // How much the glass behind apps hides the wallpaper, 0 to 1.
    pub glass_opacity: f32,
}

impl Default for System {
    fn default() -> Self {
        System { keyboard: Keyboard::default(), display: Display::default() }
    }
}

impl Default for Keyboard {
    fn default() -> Self {
        Keyboard { layout: "it".into() }
    }
}

impl Default for Display {
    fn default() -> Self {
        Display { wallpaper: "/img/wallpaper.png".into(), font_size: 0.0, glass_opacity: 0.72 }
    }
}

pub const SYSTEM_DEFAULT: &str = r#"# KrakeOS system settings. Changes apply right away (keyboard: to apps
# started after saving).

[keyboard]
# "it" (Italian QWERTY) or "us"
layout = "it"

[display]
wallpaper = "/img/wallpaper.png"
# text size in pixels, 0 = pick from the screen height
font_size = 0
# how much the glass behind apps hides the wallpaper, 0.0 to 1.0
glass_opacity = 0.72
"#;

pub fn system() -> System {
    load("system")
}
