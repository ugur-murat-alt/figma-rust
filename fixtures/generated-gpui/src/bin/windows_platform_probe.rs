use std::{env, io::Write as _, process, process::ExitCode};

use figma_gpui_runtime::configure_figma_fidelity;
use gpui::{
    App, AppContext, Bounds, Context, InteractiveElement, IntoElement, ParentElement, Render,
    StatefulInteractiveElement, Styled, Window, WindowBackgroundAppearance, WindowBounds,
    WindowOptions, div, point, px, rgba, size,
};

const GPUI_REVISION: &str = "5631830c564afa89b3aba679f45d9c3345f9460f";
const TITLE_LOADING: &str = "figma-rust-windows-probe-loading";
const TITLE_READY: &str = "figma-rust-windows-probe-ready";
const APP_ID: &str = "figma-rust-windows-probe";
const WIDTH: u32 = 160;
const HEIGHT: u32 = 80;
const LOGICAL_WIDTH: f32 = 160.0;
const LOGICAL_HEIGHT: f32 = 80.0;

struct ProbeView {
    dpi_percent: u32,
    announce_ready: bool,
}

impl Render for ProbeView {
    fn render(&mut self, window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        if self.announce_ready {
            self.announce_ready = false;
            let dpi_percent = self.dpi_percent;
            window.on_next_frame(move |window, cx| {
                let viewport = window.viewport_size();
                if (f32::from(viewport.width) - LOGICAL_WIDTH).abs() > f32::EPSILON
                    || (f32::from(viewport.height) - LOGICAL_HEIGHT).abs() > f32::EPSILON
                {
                    eprintln!("unexpected logical viewport: {viewport:?}");
                    cx.quit();
                    return;
                }
                window.activate_window();
                window.set_window_title(TITLE_READY);
                println!(
                    "{}",
                    serde_json::json!({
                        "schema_version": 1,
                        "event": "ready",
                        "pid": process::id(),
                        "window_title": TITLE_READY,
                        "app_id": APP_ID,
                        "dpi_percent": dpi_percent,
                        "logical_width": WIDTH,
                        "logical_height": HEIGHT,
                        "font_family": "Segoe UI",
                        "text_rendering": "grayscale",
                        "gpui_revision": GPUI_REVISION,
                    })
                );
                if std::io::stdout().flush().is_err() {
                    cx.quit();
                }
            });
        }

        div()
            .w(px(LOGICAL_WIDTH))
            .h(px(LOGICAL_HEIGHT))
            .bg(rgba(0xf4f6_f8ff))
            .font_family("Segoe UI")
            .text_size(px(16.0))
            .text_color(rgba(0x1020_30ff))
            .child("Aa Figma Rust")
            .id(APP_ID)
            .on_click(|_, _, cx| {
                println!(
                    "{}",
                    serde_json::json!({
                        "schema_version": 1,
                        "event": "input",
                        "pid": process::id(),
                        "kind": "primary-click",
                    })
                );
                let _ = std::io::stdout().flush();
                cx.quit();
            })
    }
}

fn main() -> ExitCode {
    let mut args = env::args().skip(1);
    let Some(flag) = args.next() else {
        eprintln!("usage: windows-platform-probe --dpi-profile PERCENT");
        return ExitCode::from(2);
    };
    let Some(value) = args.next() else {
        eprintln!("--dpi-profile requires PERCENT");
        return ExitCode::from(2);
    };
    if flag != "--dpi-profile" || args.next().is_some() {
        eprintln!("usage: windows-platform-probe --dpi-profile PERCENT");
        return ExitCode::from(2);
    }
    let Ok(dpi_percent) = value.parse::<u32>() else {
        eprintln!("DPI profile must be 100, 125, or 150");
        return ExitCode::from(2);
    };
    if !matches!(dpi_percent, 100 | 125 | 150) {
        eprintln!("DPI profile must be 100, 125, or 150");
        return ExitCode::from(2);
    }

    gpui_platform::application().run(move |cx: &mut App| {
        configure_figma_fidelity(cx);
        let result = cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds {
                    origin: point(px(0.0), px(0.0)),
                    size: size(px(LOGICAL_WIDTH), px(LOGICAL_HEIGHT)),
                })),
                focus: true,
                show: true,
                window_background: WindowBackgroundAppearance::Opaque,
                ..WindowOptions::default()
            },
            move |window, cx| {
                window.set_app_id(APP_ID);
                window.set_window_title(TITLE_LOADING);
                cx.new(|_| ProbeView {
                    dpi_percent,
                    announce_ready: true,
                })
            },
        );
        if let Err(error) = result {
            eprintln!("failed to open Windows platform probe: {error}");
            cx.quit();
        }
    });
    ExitCode::SUCCESS
}
