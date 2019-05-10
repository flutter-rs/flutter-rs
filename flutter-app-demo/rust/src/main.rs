// This build a windows app without console on windows in release mode
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod calc_channel;
mod msg_stream_channel;

use std::{env, path::PathBuf};

use fern::colors::{Color, ColoredLevelConfig};
use flutter_engine::DesktopWindowState;
use flutter_plugins::{dialog, window};
use log::info;

#[cfg(target_os = "macos")]
use core_foundation::bundle;

#[cfg(target_os = "macos")]
fn get_res_dir() -> PathBuf {
    let bd = bundle::CFBundle::main_bundle();
    let exe = bd
        .executable_url()
        .expect("Cannot get executable dir")
        .to_path()
        .expect("to_path error");
    exe.parent().unwrap().parent().unwrap().join("Resources")
}

#[cfg(not(target_os = "macos"))]
fn get_res_dir() -> PathBuf {
    env::current_exe()
        .expect("Cannot get application dir")
        .parent()
        .expect("Cannot get application dir")
        .to_path_buf()
}

fn main() {
    let colors = ColoredLevelConfig::new()
        .error(Color::Red)
        .warn(Color::Yellow)
        .info(Color::Green)
        .debug(Color::White)
        .trace(Color::BrightBlack);
    fern::Dispatch::new()
        .level(log::LevelFilter::Debug)
        .format(move |out, message, record| {
            out.finish(format_args!(
                "{}[{}][{}][{}] {}\x1B[0m",
                format_args!("\x1B[{}m", colors.get_color(&record.level()).to_fg_str()),
                chrono::Local::now().format("%H:%M:%S"),
                record.level(),
                record.target(),
                message
            ))
        })
        .chain(std::io::stdout())
        .apply()
        .unwrap();

    let (assets_path, icu_data_path) = match env::var("CARGO_MANIFEST_DIR") {
        Ok(proj_dir) => {
            info!("Running inside cargo project");
            let proj_dir = PathBuf::from(&proj_dir);
            (
                proj_dir
                    .parent()
                    .unwrap()
                    .join("build")
                    .join("flutter_assets"),
                proj_dir.join("assets/icudtl.dat"),
            )
        }
        Err(_) => {
            let res = get_res_dir();
            (res.join("flutter_assets"), res.join("icudtl.dat"))
        }
    };

    let mut engine = flutter_engine::init().unwrap();
    engine
        .create_window(
            &flutter_engine::WindowArgs {
                height: 1200,
                width: 1800,
                title: "Flutter App Demo",
                mode: flutter_engine::WindowMode::Borderless,
            },
            assets_path.to_string_lossy().to_string(),
            icu_data_path.to_string_lossy().to_string(),
            vec![],
        )
        .unwrap();
    engine.init_with_window_state(|window_state| {
        window_state
            .plugin_registrar
            .add_plugin(calc_channel::CalcPlugin::default())
            .add_plugin(dialog::DialogPlugin::default())
            .add_plugin(msg_stream_channel::MsgStreamPlugin::default())
            .add_plugin(window::WindowPlugin::default());
    });
    engine.run_window_loop(Some(&mut glfw_event_handler), None);
}

fn glfw_event_handler(window_state: &mut DesktopWindowState, event: glfw::WindowEvent) -> bool {
    match event {
        glfw::WindowEvent::CursorPos(x, y) => {
            let dragging = window_state.with_window_and_plugin_mut_result(
                |window, p: &mut window::WindowPlugin| p.drag_window(window, x, y),
            );
            if let Some(dragging) = dragging {
                !dragging
            } else {
                true
            }
        }
        _ => true,
    }
}
