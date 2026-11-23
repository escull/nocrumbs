// SPDX-License-Identifier: GPL-3.0-or-later
use std::sync::Arc;

use m0_ghost_bar::{App, setup};
use nocrumbs_config::FileStore;

fn main() -> eframe::Result {
    // `--small` uses a 1,000-entry /big folder instead of 100,000.
    // `--start /big` opens in the 100,000-entry folder.
    // `--state FILE` keeps aliases and frecency in FILE (TOML) between runs; without it nothing is saved.
    let args: Vec<String> = std::env::args().collect();
    let big_n = if args.iter().any(|a| a == "--small") { 1_000 } else { 100_000 };
    let start = args.windows(2).find(|w| w[0] == "--start").map_or("/home/user".to_string(), |w| w[1].clone());
    let state = args.windows(2).find(|w| w[0] == "--state").map(|w| w[1].clone());
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([980.0, 640.0]).with_title("NoCrumbs — M0 spike"),
        ..Default::default()
    };
    eframe::run_native(
        "nocrumbs-m0-spike",
        options,
        Box::new(move |cc| {
            setup(&cc.egui_ctx);
            let mut app = App::with_start(big_n, &start);
            if let Some(path) = &state {
                app.attach_store(Arc::new(FileStore::new(path)));
            }
            Ok(Box::new(app))
        }),
    )
}
