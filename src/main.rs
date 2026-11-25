use multitime::{doc::Doc, ui::App};
use std::path::PathBuf;

fn main() -> eframe::Result {
    let path = PathBuf::from(std::env::args().nth(1).unwrap_or_else(|| "solve.txt".to_owned()));
    let doc = match std::fs::read_to_string(&path) {
        Ok(s) => Doc::from_file(&s).unwrap_or_else(|e| {
            eprintln!("{}: {e}", path.display());
            std::process::exit(1)
        }),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Doc::default(),
        Err(e) => {
            eprintln!("{}: {e}", path.display());
            std::process::exit(1)
        }
    };
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default().with_inner_size([1200.0, 800.0]),
        ..Default::default()
    };
    eframe::run_native(
        "multitime",
        options,
        Box::new(|_| Ok(Box::new(App::new(doc, path)))),
    )
}
