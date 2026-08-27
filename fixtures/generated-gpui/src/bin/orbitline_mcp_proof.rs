use std::{
    cell::Cell,
    env,
    ffi::OsString,
    fs::{File, OpenOptions},
    io::{self, Read as _, Write as _},
    path::{Path, PathBuf},
    process::{self, ExitCode},
    rc::Rc,
    time::{SystemTime, UNIX_EPOCH},
};

use base64::Engine as _;
use figma_gpui_runtime::configure_figma_fidelity;
use gpui::{
    App, AppContext, Bounds, Context, Div, FontWeight, IntoElement, ParentElement, Render, Styled,
    Window, WindowBackgroundAppearance, WindowBounds, WindowOptions, div, point, px, rgb, size,
};
use image::GenericImageView as _;

const GPUI_REVISION: &str = "5631830c564afa89b3aba679f45d9c3345f9460f";
const WM_CLASS: &str = "figma-rust-orbitline-mcp";
const PNG_DATA_URL_PREFIX: &str = "data:image/png;base64,";
const MAX_DATA_URL_BYTES: u64 = 16 * 1024 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Target {
    Table,
    Dialog,
}

impl Target {
    fn parse(value: &OsString) -> Result<Self, String> {
        match value.to_str() {
            Some("table") => Ok(Self::Table),
            Some("dialog") => Ok(Self::Dialog),
            Some(value) => Err(format!("target must be `table` or `dialog`, got {value:?}")),
            None => Err("target must be valid UTF-8".to_owned()),
        }
    }

    fn name(self) -> &'static str {
        match self {
            Self::Table => "table",
            Self::Dialog => "dialog",
        }
    }

    fn node_id(self) -> &'static str {
        match self {
            Self::Table => "516:223",
            Self::Dialog => "518:96",
        }
    }

    fn dimensions(self) -> (u32, u32) {
        match self {
            Self::Table => (520, 176),
            Self::Dialog => (460, 220),
        }
    }

    fn logical_dimensions(self) -> (f32, f32) {
        match self {
            Self::Table => (520.0, 176.0),
            Self::Dialog => (460.0, 220.0),
        }
    }

    fn loading_title(self) -> &'static str {
        match self {
            Self::Table => "figma-rust-orbitline-table-loading",
            Self::Dialog => "figma-rust-orbitline-dialog-loading",
        }
    }

    fn ready_title(self) -> &'static str {
        match self {
            Self::Table => "figma-rust-orbitline-table-ready",
            Self::Dialog => "figma-rust-orbitline-dialog-ready",
        }
    }
}

enum Command {
    Display(Target),
    Ingest { target: Target, output: PathBuf },
}

struct ProofView {
    target: Target,
    announce_ready: bool,
    ready: Rc<Cell<bool>>,
}

impl Render for ProofView {
    fn render(&mut self, window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        if self.announce_ready {
            self.announce_ready = false;
            let target = self.target;
            let ready = Rc::clone(&self.ready);
            window.on_next_frame(move |window, cx| {
                let viewport = window.viewport_size();
                let (width, height) = target.dimensions();
                let (logical_width, logical_height) = target.logical_dimensions();
                if viewport.width != px(logical_width) || viewport.height != px(logical_height) {
                    eprintln!(
                        "GPUI viewport is {}x{}, expected {width}x{height}",
                        f32::from(viewport.width),
                        f32::from(viewport.height)
                    );
                    cx.quit();
                    return;
                }

                window.set_window_title(target.ready_title());
                let event = serde_json::json!({
                    "schema_version": 1,
                    "event": "ready",
                    "pid": process::id(),
                    "target": target.name(),
                    "figma_file_key": "AShHVrUqsKDWavqH8D4S09",
                    "figma_node_id": target.node_id(),
                    "window_title": target.ready_title(),
                    "wm_class": WM_CLASS,
                    "logical_width": width,
                    "logical_height": height,
                    "gpui_revision": GPUI_REVISION,
                    "text_rendering": "grayscale",
                });
                println!("{event}");
                if let Err(error) = io::stdout().flush() {
                    eprintln!("failed to publish readiness: {error}");
                    cx.quit();
                } else {
                    ready.set(true);
                }
            });
        }

        match self.target {
            Target::Table => render_table(),
            Target::Dialog => render_dialog(),
        }
    }
}

fn render_table() -> Div {
    div()
        .flex()
        .flex_col()
        .items_start()
        .w(px(520.0))
        .h(px(176.0))
        .overflow_hidden()
        .bg(rgb(0x0007_090b))
        .border_1()
        .border_color(rgb(0x001a_222a))
        .rounded(px(4.0))
        .child(table_row(true))
        .child(table_row(false))
        .child(table_row(false))
        .child(table_row(false))
        .child(table_row(false))
}

fn table_row(header: bool) -> Div {
    let height = if header { 24.0 } else { 28.0 };
    let background = if header { 0x0011_161b } else { 0x0007_090b };
    let values = if header {
        ["Symbol", "Name", "Price", "Change", "Volume", "Mkt Cap"]
    } else {
        ["AAPL", "Apple Inc.", "$189.84", "+1.25%", "52.3M", "$2.98T"]
    };

    div()
        .flex()
        .flex_none()
        .items_center()
        .justify_between()
        .w(px(520.0))
        .h(px(height))
        .px(px(8.0))
        .bg(rgb(background))
        .border_b_1()
        .border_color(rgb(0x001a_222a))
        .child(table_cell(values[0], 64.0, false, header, height))
        .child(table_cell(values[1], 112.0, false, header, height))
        .child(table_cell(values[2], 80.0, true, header, height))
        .child(table_cell(values[3], 80.0, true, header, height))
        .child(table_cell(values[4], 80.0, true, header, height))
        .child(table_cell(values[5], 88.0, true, header, height))
}

fn table_cell(value: &'static str, width: f32, numeric: bool, header: bool, height: f32) -> Div {
    let mut cell = div()
        .flex()
        .flex_none()
        .items_center()
        .w(px(width))
        .h(px(height))
        .px(px(8.0))
        .overflow_hidden()
        .font_family("JetBrains Mono")
        .font_weight(FontWeight(if header { 500.0 } else { 400.0 }))
        .text_size(px(if header { 10.0 } else { 11.0 }))
        .line_height(px(if header { 12.0 } else { 14.0 }))
        .text_color(rgb(if header { 0x007b_8793 } else { 0x00a1_abb5 }));
    if numeric {
        cell = cell.justify_end();
    }
    cell.child(value)
}

fn render_dialog() -> Div {
    div()
        .flex()
        .flex_col()
        .items_start()
        .gap(px(10.0))
        .w(px(460.0))
        .h(px(220.0))
        .px(px(16.0))
        .py(px(14.0))
        .overflow_hidden()
        .bg(rgb(0x0015_1b21))
        .child(
            div()
                .flex_none()
                .w(px(428.0))
                .h(px(16.0))
                .font_family("Inter")
                .font_weight(FontWeight(400.0))
                .text_size(px(10.0))
                .line_height(px(13.0))
                .text_color(rgb(0x00a1_abb5))
                .child(
                    "THYAO Analiz workspace’inde kaydedilmemiş panel boyutları ve gösterge değişiklikleri bulunuyor.",
                ),
        )
        .child(
            div()
                .flex()
                .flex_none()
                .flex_col()
                .items_start()
                .gap(px(5.0))
                .w(px(428.0))
                .h(px(96.0))
                .p(px(10.0))
                .overflow_hidden()
                .bg(rgb(0x0007_090b))
                .border_1()
                .border_color(rgb(0x001a_222a))
                .rounded(px(2.0))
                .child(metric_row("Panel bölünmesi", "3 modül"))
                .child(metric_row("Gösterge değişikliği", "EMA 20 → 34"))
                .child(metric_row("Çizim taslağı", "1 trend çizgisi")),
        )
        .child(
            div()
                .flex()
                .flex_none()
                .items_start()
                .gap(px(8.0))
                .w(px(428.0))
                .h(px(34.0))
                .px(px(8.0))
                .py(px(5.0))
                .overflow_hidden()
                .bg(rgb(0x000d_1115))
                .border_1()
                .border_color(rgb(0x001a_222a))
                .rounded(px(2.0))
                .child(
                    div()
                        .flex_none()
                        .w(px(16.0))
                        .h(px(16.0))
                        .bg(rgb(0x0011_161b))
                        .border_1()
                        .border_color(rgb(0x0028_333e))
                        .rounded(px(2.0)),
                )
                .child(
                    div()
                        .flex_none()
                        .font_family("Inter")
                        .font_weight(FontWeight(500.0))
                        .text_size(px(9.0))
                        .line_height(px(12.0))
                        .text_color(rgb(0x00a1_abb5))
                        .child("Bu workspace için bir daha sorma"),
                ),
        )
}

fn metric_row(label: &'static str, value: &'static str) -> Div {
    div()
        .flex()
        .flex_none()
        .items_start()
        .w(px(408.0))
        .h(px(24.0))
        .overflow_hidden()
        .child(
            div()
                .flex_none()
                .font_family("Inter")
                .font_weight(FontWeight(400.0))
                .text_size(px(9.0))
                .line_height(px(12.0))
                .text_color(rgb(0x007b_8793))
                .child(label),
        )
        .child(div().flex_1().h(px(12.0)))
        .child(
            div()
                .flex_none()
                .font_family("JetBrains Mono")
                .font_weight(FontWeight(500.0))
                .text_size(px(9.0))
                .line_height(px(12.0))
                .text_color(rgb(0x00e8_ebee))
                .child(value),
        )
}

fn parse_command(args: impl IntoIterator<Item = OsString>) -> Result<Command, String> {
    let mut args = args.into_iter();
    let Some(command) = args.next() else {
        return Err("expected `--display TARGET` or `--ingest-data-url TARGET OUTPUT`".to_owned());
    };
    let target = Target::parse(
        &args
            .next()
            .ok_or_else(|| "capture command requires TARGET".to_owned())?,
    )?;
    let parsed = match command.to_str() {
        Some("--display") => Command::Display(target),
        Some("--ingest-data-url") => Command::Ingest {
            target,
            output: args
                .next()
                .ok_or_else(|| "--ingest-data-url requires OUTPUT".to_owned())?
                .into(),
        },
        Some(command) => return Err(format!("unknown command: {command}")),
        None => return Err("capture command must be valid UTF-8".to_owned()),
    };
    if args.next().is_some() {
        return Err("capture command received unexpected trailing arguments".to_owned());
    }
    Ok(parsed)
}

fn run_display(target: Target) -> ExitCode {
    let ready = Rc::new(Cell::new(false));
    let app_ready = Rc::clone(&ready);
    gpui_platform::application().run(move |cx: &mut App| {
        configure_figma_fidelity(cx);
        let view_ready = Rc::clone(&app_ready);
        let (logical_width, logical_height) = target.logical_dimensions();
        let result = cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds {
                    origin: point(px(0.0), px(0.0)),
                    size: size(px(logical_width), px(logical_height)),
                })),
                focus: true,
                show: true,
                window_background: WindowBackgroundAppearance::Transparent,
                ..WindowOptions::default()
            },
            move |window, cx| {
                window.set_app_id(WM_CLASS);
                window.set_window_title(target.loading_title());
                cx.new(|_| ProofView {
                    target,
                    announce_ready: true,
                    ready: view_ready,
                })
            },
        );
        if let Err(error) = result {
            eprintln!("failed to open OrbitLine proof window: {error}");
            cx.quit();
        }
    });
    if ready.get() {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(1)
    }
}

fn ingest_data_url(target: Target, output: &Path) -> Result<(), String> {
    let mut input = String::new();
    io::stdin()
        .take(MAX_DATA_URL_BYTES + 1)
        .read_to_string(&mut input)
        .map_err(|error| format!("failed to read PNG data URL: {error}"))?;
    if input.len() as u64 > MAX_DATA_URL_BYTES {
        return Err(format!(
            "PNG data URL exceeds the {MAX_DATA_URL_BYTES}-byte limit"
        ));
    }
    let encoded = input
        .trim()
        .strip_prefix(PNG_DATA_URL_PREFIX)
        .ok_or_else(|| format!("capture input must start with {PNG_DATA_URL_PREFIX}"))?;
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(encoded)
        .map_err(|error| format!("failed to decode PNG data URL: {error}"))?;
    let image = image::load_from_memory_with_format(&bytes, image::ImageFormat::Png)
        .map_err(|error| format!("failed to decode capture PNG: {error}"))?;
    if image.dimensions() != target.dimensions() {
        return Err(format!(
            "{} capture must be {:?}, got {:?}",
            target.name(),
            target.dimensions(),
            image.dimensions()
        ));
    }

    let directory = output
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let lock = File::open(directory).map_err(|error| error.to_string())?;
    lock.try_lock().map_err(|error| error.to_string())?;
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| error.to_string())?
        .as_nanos();
    let name = output.file_name().unwrap_or_default().to_string_lossy();
    let temporary = output.with_file_name(format!(".{name}.tmp-{}-{nonce}", process::id()));
    let result = (|| {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)
            .map_err(|error| error.to_string())?;
        file.write_all(&bytes).map_err(|error| error.to_string())?;
        file.sync_all().map_err(|error| error.to_string())?;
        std::fs::rename(&temporary, output).map_err(|error| error.to_string())
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&temporary);
    }
    result?;
    println!(
        "{}",
        serde_json::json!({
            "schema_version": 1,
            "event": "ingested",
            "target": target.name(),
            "output": output,
            "width": image.width(),
            "height": image.height(),
        })
    );
    Ok(())
}

fn main() -> ExitCode {
    let command = match parse_command(env::args_os().skip(1)) {
        Ok(command) => command,
        Err(error) => {
            eprintln!("{error}");
            return ExitCode::from(2);
        }
    };
    match command {
        Command::Display(target) => run_display(target),
        Command::Ingest { target, output } => match ingest_data_url(target, &output) {
            Ok(()) => ExitCode::SUCCESS,
            Err(error) => {
                eprintln!("{error}");
                ExitCode::from(1)
            }
        },
    }
}

#[cfg(test)]
mod tests {
    use std::ffi::OsString;

    use super::{Command, Target, parse_command};

    #[test]
    fn parses_isolated_proof_commands() {
        let command = parse_command([OsString::from("--display"), OsString::from("table")]);
        assert!(matches!(command, Ok(Command::Display(Target::Table))));

        let command = parse_command([
            OsString::from("--ingest-data-url"),
            OsString::from("dialog"),
            OsString::from("actual.dialog.png"),
        ]);
        assert!(matches!(
            command,
            Ok(Command::Ingest {
                target: Target::Dialog,
                ..
            })
        ));
    }

    #[test]
    fn targets_match_figma_screenshot_dimensions() {
        assert_eq!(Target::Table.dimensions(), (520, 176));
        assert_eq!(Target::Dialog.dimensions(), (460, 220));
    }
}
