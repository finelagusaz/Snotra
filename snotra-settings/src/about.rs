use eframe::egui;

struct AboutApp;

impl eframe::App for AboutApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Close on Escape
        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.vertical_centered(|ui| {
                ui.add_space(24.0);
                ui.heading("Snotra");
                ui.add_space(8.0);

                let version = env!("CARGO_PKG_VERSION");
                ui.label(format!("v{version}"));
                ui.add_space(24.0);

                ui.label("Fine Lagusaz");
                ui.add_space(8.0);

                if ui.link("algiz.rune@gmail.com").clicked() {
                    let _ = open::that("mailto:algiz.rune@gmail.com?subject=Snotra%E3%81%AB%E3%81%A4%E3%81%84%E3%81%A6");
                }
                if ui.link("https://blankrune.sakura.ne.jp/").clicked() {
                    let _ = open::that("https://blankrune.sakura.ne.jp/");
                }
            });
        });
    }
}

pub fn run() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Snotra について")
            .with_inner_size([400.0, 300.0])
            .with_resizable(false)
            .with_maximize_button(false),
        ..Default::default()
    };

    eframe::run_native(
        "Snotra について",
        options,
        Box::new(|cc| {
            crate::font::configure_fonts(&cc.egui_ctx);
            Ok(Box::new(AboutApp))
        }),
    )
}
