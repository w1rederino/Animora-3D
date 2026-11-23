//! Animora 3D – a 3D modeling and animation app written in Rust.
//!
//! Start options:
//!   Animora3D.exe                     open normally
//!   Animora3D.exe <datei>             open a scene (.anim3d) or import a
//!                                     .cast/.xanim_export/.xmodel_export
//!   Animora3D.exe --screenshot <png>  draw ~120 frames, save a picture, exit
//!   Animora3D.exe --write-icon <ico>  write the app icon as a .ico file

// No console window, also when started from Explorer with a debug build.
#![windows_subsystem = "windows"]

mod anim;
mod app;
mod brand;
mod cast;
mod cast_app;
mod clips;
mod cod;
mod editors;
mod i18n;
mod icons;
mod math;
mod mesh;
mod panels;
mod render;
mod rig;
mod scene;
mod tools;
mod view;
mod weapon;

use std::path::PathBuf;

fn main() -> eframe::Result<()> {
    i18n::load_saved();
    let args: Vec<String> = std::env::args().skip(1).collect();
    let value_of = |flag: &str| args.iter().position(|a| a == flag).and_then(|i| args.get(i + 1)).map(PathBuf::from);
    let shot = value_of("--screenshot");
    if let Some(ico) = value_of("--write-icon") {
        if let Err(e) = brand::write_ico(&ico) {
            eprintln!("{}", crate::trf!("Symbol konnte nicht geschrieben werden: {e}", "Icon could not be written: {e}"));
        }
        return Ok(());
    }
    // files given on the command line (Explorer "Open with" / several files
    // dragged onto the icon): models first, every animation becomes a clip
    let mut open: Vec<PathBuf> = Vec::new();
    let mut skip = false;
    for a in &args {
        if skip {
            skip = false;
            continue;
        }
        if a == "--screenshot" || a == "--write-icon" {
            skip = true;
            continue;
        }
        if !a.starts_with("--") {
            open.push(PathBuf::from(a));
        }
    }

    let icon = egui::IconData { rgba: brand::icon_rgba(64), width: 64, height: 64 };
    let mut viewport = egui::ViewportBuilder::default()
        .with_title("Animora 3D")
        .with_app_id("animora3d")
        .with_icon(icon)
        .with_inner_size([1600.0, 960.0])
        .with_min_inner_size([900.0, 600.0]);
    if shot.is_some() {
        // Open quietly for automated checks: no focus, and behind other windows on Windows.
        viewport = viewport.with_active(false);
        #[cfg(target_os = "windows")]
        {
            viewport = viewport.with_window_level(egui::WindowLevel::AlwaysOnBottom);
        }
    }
    #[allow(unused_mut)]
    let mut options = eframe::NativeOptions { viewport, ..Default::default() };
    #[cfg(target_os = "macos")]
    if shot.is_some() {
        options.event_loop_builder = Some(Box::new(|b| {
            use winit::platform::macos::EventLoopBuilderExtMacOS;
            b.with_activate_ignoring_other_apps(false);
        }));
    }
    eframe::run_native(
        "Animora 3D",
        options,
        Box::new(move |cc| {
            let mut app = app::App::new(&cc.egui_ctx, shot);
            match open.len() {
                0 => {}
                1 => app.open_any(&open[0]),
                _ => {
                    let msg = app.import_files(open);
                    app.status = msg;
                }
            }
            Ok(Box::new(app))
        }),
    )
}
