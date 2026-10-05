#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
mod runner;

use eframe::egui::{self, Color32, RichText, Stroke};
use runner::{Job, Settings};
use std::{
    path::{Path, PathBuf},
    sync::atomic::Ordering,
    time::{Duration, Instant},
};

const INK: Color32 = Color32::from_rgb(26, 45, 55);
const MUTED: Color32 = Color32::from_rgb(85, 106, 116);
const GREEN: Color32 = Color32::from_rgb(18, 109, 90);
const MINT: Color32 = Color32::from_rgb(228, 244, 235);
const PAPER: Color32 = Color32::from_rgb(245, 247, 245);
const LINE: Color32 = Color32::from_rgb(217, 227, 223);

fn main() -> eframe::Result {
    let size = if cfg!(feature = "ui-capture") && std::env::var_os("RADD_UI_SMALL").is_some() {
        [940.0, 720.0]
    } else {
        [1220.0, 880.0]
    };
    eframe::run_native(
        "RADD Desktop",
        eframe::NativeOptions {
            viewport: egui::ViewportBuilder::default()
                .with_inner_size(size)
                .with_min_inner_size([900.0, 680.0]),
            persist_window: !cfg!(feature = "ui-capture"),
            persistence_path: if cfg!(feature = "ui-capture") {
                std::env::var_os("RADD_UI_CAPTURE_DIR")
                    .map(|p| PathBuf::from(p).join("window-state.ron"))
            } else {
                None
            },
            ..Default::default()
        },
        Box::new(|cc| Ok(Box::new(Desktop::new(cc)))),
    )
}

#[derive(Clone, Copy, PartialEq)]
enum Page {
    Files,
    Review,
    Results,
    Guide,
}

struct Desktop {
    settings: Settings,
    inputs: Vec<PathBuf>,
    config: Option<PathBuf>,
    job: Option<Job>,
    message: String,
    is_error: bool,
    run_error: bool,
    page: Page,
    result_tab: usize,
    directory: Option<PathBuf>,
    files: Vec<PathBuf>,
    selected: Option<PathBuf>,
    preview: String,
    log: String,
    last_log: Instant,
    search: String,
    settings_open: bool,
    engine_check: bool,
    #[cfg(feature = "ui-capture")]
    capture: Option<Capture>,
}

impl Desktop {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let mut style = (*cc.egui_ctx.style()).clone();
        style.visuals = egui::Visuals::light();
        style.visuals.override_text_color = Some(INK);
        style.visuals.weak_text_color = Some(MUTED);
        style.visuals.panel_fill = PAPER;
        style.visuals.window_fill = Color32::WHITE;
        style.visuals.extreme_bg_color = Color32::WHITE;
        style.visuals.faint_bg_color = MINT;
        style.visuals.selection.bg_fill = Color32::from_rgb(188, 223, 208);
        style.visuals.selection.stroke = Stroke::new(1.0, GREEN);
        style.visuals.widgets.inactive.bg_fill = Color32::WHITE;
        style.visuals.widgets.inactive.weak_bg_fill = Color32::WHITE;
        style.visuals.widgets.inactive.bg_stroke = Stroke::new(1.0, LINE);
        style.visuals.widgets.hovered.bg_fill = MINT;
        style.visuals.widgets.hovered.weak_bg_fill = MINT;
        style.visuals.widgets.hovered.bg_stroke = Stroke::new(1.0, GREEN);
        style.visuals.widgets.active.bg_fill = MINT;
        style.visuals.widgets.noninteractive.fg_stroke = Stroke::new(1.0, INK);
        style.spacing.item_spacing = egui::vec2(10.0, 12.0);
        style.spacing.button_padding = egui::vec2(18.0, 12.0);
        style.spacing.interact_size.y = 38.0;
        style.spacing.scroll.floating = false;
        for (kind, size) in [
            (egui::TextStyle::Body, 16.0),
            (egui::TextStyle::Button, 16.0),
            (egui::TextStyle::Heading, 30.0),
            (egui::TextStyle::Small, 13.0),
        ] {
            style
                .text_styles
                .insert(kind, egui::FontId::proportional(size));
        }
        cc.egui_ctx.set_style(style);
        let mut settings: Settings = cc
            .storage
            .and_then(|s| eframe::get_value(s, "settings-v1"))
            .unwrap_or_default();
        if !settings.engine.is_file() {
            settings.engine = runner::find_engine().unwrap_or_default();
        }
        let app = Self {
            settings,
            inputs: vec![],
            config: None,
            job: None,
            message: String::new(),
            is_error: false,
            run_error: false,
            page: Page::Files,
            result_tab: 0,
            directory: None,
            files: vec![],
            selected: None,
            preview: String::new(),
            log: String::new(),
            last_log: Instant::now(),
            search: String::new(),
            settings_open: false,
            engine_check: false,
            #[cfg(feature = "ui-capture")]
            capture: None,
        };
        #[cfg(feature = "ui-capture")]
        let app = app.with_capture();
        app
    }

    fn report(&mut self, result: Result<(), String>) {
        if let Err(e) = result {
            self.message = e;
            self.is_error = true;
        }
    }

    fn launch(&mut self, check: bool) {
        match runner::start(
            self.settings.clone(),
            self.inputs.clone(),
            self.config.clone(),
            check,
        ) {
            Ok(job) => {
                self.directory = Some(job.directory.clone());
                self.job = Some(job);
                self.files.clear();
                self.selected = None;
                self.preview.clear();
                self.log.clear();
                self.search.clear();
                self.is_error = false;
                self.result_tab = 1;
                self.engine_check = check;
                self.run_error = false;
                self.page = Page::Results;
                self.message = if check {
                    "Checking which processor types your engine can read…"
                } else {
                    "RADD is reading your file and translating its instructions."
                }
                .into();
            }
            Err(e) => self.report(Err(e)),
        }
    }

    fn poll(&mut self, ctx: &egui::Context) {
        if let Some(job) = &self.job {
            ctx.request_repaint_after(Duration::from_millis(150));
            let done = match job.receiver.try_recv() {
                Ok(result) => Some(result),
                Err(std::sync::mpsc::TryRecvError::Disconnected) => Some(Err(
                    "The analysis stopped unexpectedly. Open the Analysis log for details.".into(),
                )),
                Err(_) => None,
            };
            if self.last_log.elapsed() > Duration::from_millis(500) || done.is_some() {
                self.log = runner::read_preview(&job.directory.join("engine.log"), true);
                self.last_log = Instant::now();
            }
            if let Some(result) = done {
                self.is_error = result.is_err();
                self.run_error = self.is_error;
                self.message = result.unwrap_or_else(|e| {
                    if e.contains("did not produce") {
                        "No readable instructions were found. Try the sample, or check that you selected a compiled program. The Analysis log has more details.".into()
                    } else { e }
                });
                self.files = runner::result_files(&job.directory);
                if let Some(file) = self
                    .files
                    .iter()
                    .find(|p| p.extension().is_some_and(|e| e == "asm"))
                    .or(self.files.first())
                    .cloned()
                {
                    self.preview = runner::read_preview(&file, false);
                    self.selected = Some(file);
                    if !self.is_error {
                        self.result_tab = 0;
                    }
                }
                self.job = None;
            }
        }
    }

    fn add_inputs(&mut self, paths: Vec<PathBuf>) {
        for path in paths {
            if path.is_file() && !self.inputs.contains(&path) {
                self.inputs.push(path);
            }
        }
        self.page = Page::Files;
        self.message.clear();
        self.is_error = false;
    }

    fn sample_path() -> Option<PathBuf> {
        let mut candidates = vec![];
        if let Ok(exe) = std::env::current_exe() {
            if let Some(dir) = exe.parent() {
                candidates.push(dir.join("samples/hello_x32"));
            }
        }
        if let Ok(cwd) = std::env::current_dir() {
            candidates.push(cwd.join("samples/hello_x32"));
        }
        candidates.into_iter().find(|p| p.is_file())
    }

    fn load_sample(&mut self) {
        if let Some(sample) = Self::sample_path() {
            self.add_inputs(vec![sample]);
        } else {
            self.report(Err("The sample is missing. Extract the complete download, including the samples folder, or choose a file of your own.".into()));
        }
    }

    fn browse(&mut self) {
        if let Some(paths) = rfd::FileDialog::new()
            .set_title("Choose a compiled program or library")
            .pick_files()
        {
            self.add_inputs(paths);
        }
    }

    fn sidebar(&mut self, ui: &mut egui::Ui) {
        ui.visuals_mut().override_text_color = Some(Color32::WHITE);
        ui.visuals_mut().weak_text_color = Some(Color32::from_rgb(180, 201, 202));
        ui.add_space(12.0);
        ui.horizontal(|ui| {
            egui::Frame::new()
                .fill(Color32::from_rgb(159, 223, 192))
                .corner_radius(10)
                .inner_margin(10)
                .show(ui, |ui| {
                    ui.label(RichText::new("R").size(25.0).strong().color(INK));
                });
            ui.vertical(|ui| {
                ui.label(
                    RichText::new("RADD")
                        .size(25.0)
                        .strong()
                        .color(Color32::WHITE),
                );
                ui.label(
                    RichText::new("DESKTOP  /  0.2")
                        .size(11.0)
                        .color(Color32::from_rgb(180, 201, 202)),
                );
            });
        });
        ui.add_space(36.0);
        ui.label(
            RichText::new("YOUR WORKSPACE")
                .size(11.0)
                .color(Color32::from_rgb(160, 187, 189)),
        );
        ui.add_space(6.0);
        for (page, number, title, subtitle) in [
            (Page::Files, "01", "Choose a file", "Start with a program"),
            (
                Page::Review,
                "02",
                "Review & analyze",
                "Choose where to save",
            ),
            (Page::Results, "03", "Explore results", "See what is inside"),
        ] {
            let active = self.page == page;
            let available = page == Page::Files
                || (page == Page::Review && !self.inputs.is_empty())
                || (page == Page::Results && self.directory.is_some());
            let (rect, response) = ui
                .allocate_exact_size(egui::vec2(ui.available_width(), 76.0), egui::Sense::click());
            if active || (response.hovered() && available) {
                ui.painter()
                    .rect_filled(rect, 10.0, Color32::from_rgb(43, 71, 75));
            }
            if active {
                ui.painter().rect_filled(
                    egui::Rect::from_min_size(
                        rect.left_top() + egui::vec2(0.0, 14.0),
                        egui::vec2(3.0, 48.0),
                    ),
                    2.0,
                    Color32::from_rgb(159, 223, 192),
                );
            }
            let fg = if active || available {
                Color32::WHITE
            } else {
                Color32::from_rgb(145, 168, 172)
            };
            ui.painter().text(
                rect.left_top() + egui::vec2(14.0, 19.0),
                egui::Align2::LEFT_TOP,
                number,
                egui::FontId::monospace(12.0),
                Color32::from_rgb(159, 223, 192),
            );
            ui.painter().text(
                rect.left_top() + egui::vec2(44.0, 15.0),
                egui::Align2::LEFT_TOP,
                title,
                egui::FontId::proportional(16.0),
                fg,
            );
            ui.painter().text(
                rect.left_top() + egui::vec2(44.0, 41.0),
                egui::Align2::LEFT_TOP,
                subtitle,
                egui::FontId::proportional(12.0),
                Color32::from_rgb(180, 201, 202),
            );
            response.widget_info(|| {
                egui::WidgetInfo::selected(egui::WidgetType::Button, available, active, title)
            });
            if response.clicked() && available {
                self.page = page;
            }
            if !available {
                response.on_hover_text(if page == Page::Review {
                    "Choose a file first."
                } else {
                    "Your results appear here after analysis."
                });
            }
        }
        ui.add_space(24.0);
        if ui
            .add(
                egui::Button::new(RichText::new("Beginner's guide").color(INK))
                    .fill(Color32::from_rgb(214, 233, 223))
                    .min_size(egui::vec2(ui.available_width(), 42.0)),
            )
            .clicked()
        {
            self.page = Page::Guide;
        }
        ui.with_layout(egui::Layout::bottom_up(egui::Align::LEFT), |ui| {
            ui.add_space(8.0);
            if ui
                .add(
                    egui::Button::new(RichText::new("Engine & settings").color(INK))
                        .fill(Color32::from_rgb(227, 235, 231))
                        .min_size(egui::vec2(ui.available_width(), 40.0)),
                )
                .clicked()
            {
                self.settings_open = true;
            }
            ui.horizontal(|ui| {
                let color = if self.settings.engine.is_file() {
                    Color32::from_rgb(159, 223, 192)
                } else {
                    Color32::from_rgb(250, 198, 133)
                };
                let (dot, _) = ui.allocate_exact_size(egui::vec2(12.0, 12.0), egui::Sense::hover());
                ui.painter().circle_filled(dot.center(), 4.0, color);
                ui.label(
                    RichText::new(if self.settings.engine.is_file() {
                        "Engine located"
                    } else {
                        "Engine needs setup"
                    })
                    .size(13.0)
                    .color(color),
                );
            });
            ui.label(
                RichText::new("Your files stay on this computer.")
                    .size(12.0)
                    .color(Color32::from_rgb(180, 201, 202)),
            );
        });
    }

    fn files_page(&mut self, ui: &mut egui::Ui) {
        heading(
            ui,
            "STEP 01 / CHOOSE A FILE",
            "Let's look inside a program.",
            "RADD translates a program's machine instructions into readable text. You don't need to know any commands to get started.",
        );
        if self.inputs.is_empty() {
            card(MINT).show(ui, |ui| {
                ui.horizontal(|ui| {
                    file_art(ui, 78.0);
                    ui.vertical(|ui| {
                        ui.label(RichText::new("New here? Try a sample.").size(22.0).strong());
                        ui.label("We've included a small example so you can learn the steps.");
                        if primary(ui, "Use the sample file", true).clicked() {
                            self.load_sample();
                        }
                    });
                });
            });
            ui.add_space(8.0);
        }
        card(Color32::WHITE).show(ui, |ui| {
            eyebrow(ui, if self.inputs.is_empty() { "OR BRING YOUR OWN" } else { "YOUR SELECTED FILES" });
            ui.label(RichText::new(if self.inputs.is_empty() { "Choose a program to explore" } else { "You're off to a good start." }).size(22.0).strong());
            ui.label("Choose a compiled program or library, such as an .exe or .dll. You can also drag files into this window.");
            ui.horizontal_wrapped(|ui| {
                if ui.button(if self.inputs.is_empty() { "Browse for a file…" } else { "Add another file…" }).clicked() { self.browse(); }
                if !self.inputs.is_empty() && ui.button("Clear selection").clicked() { self.inputs.clear(); }
            });
            let mut remove = None;
            for (i, path) in self.inputs.iter().enumerate() {
                ui.separator();
                ui.horizontal(|ui| {
                    ui.label(RichText::new("FILE").size(11.0).color(GREEN));
                    ui.vertical(|ui| {
                        ui.label(RichText::new(path.file_name().unwrap_or_default().to_string_lossy()).strong()).on_hover_text(path.display().to_string());
                        let size = std::fs::metadata(path).map(|m| format!("{:.1} KB", m.len() as f64 / 1024.0)).unwrap_or_else(|_| "File missing — choose it again".into());
                        ui.label(RichText::new(size).small().color(MUTED));
                    });
                    if ui.small_button("Remove").clicked() { remove = Some(i); }
                });
            }
            if let Some(i) = remove { self.inputs.remove(i); }
            if !self.inputs.is_empty() { ui.add_space(4.0); note(ui, "Next: choose where your results will be saved, then start the analysis."); }
        });
        ui.add_space(8.0);
        ui.columns(2, |cols| {
            tip(&mut cols[0], "What is a binary?", "A compiled program is sometimes called a binary. RADD reads its bytes and shows the instructions inside.");
            tip(&mut cols[1], "Your original stays intact", "RADD reads the selected file; it does not run or change it. Results are saved separately.");
        });
    }

    fn review_page(&mut self, ui: &mut egui::Ui) {
        heading(
            ui,
            "STEP 02 / REVIEW & ANALYZE",
            "A quick check, then you're ready.",
            "The recommended settings are a good place to start. You can leave the technical options as they are.",
        );
        if !self.settings.engine.is_file() {
            card(Color32::from_rgb(255,243,220)).show(ui, |ui| {
                ui.label(RichText::new("One thing to set up first").strong().color(Color32::from_rgb(125,78,22)));
                ui.label("The engine is the part of RADD that reads your files. Keep radd and radd-desktop together in the extracted download.");
                if ui.button("Find the engine").clicked() { self.settings_open = true; }
            });
        }
        card(Color32::WHITE).show(ui, |ui| {
            eyebrow(ui, "WHAT YOU'RE ANALYZING");
            ui.horizontal_wrapped(|ui| {
                ui.label(
                    RichText::new(format!(
                        "{} {} selected",
                        self.inputs.len(),
                        if self.inputs.len() == 1 {
                            "file"
                        } else {
                            "files"
                        }
                    ))
                    .size(22.0)
                    .strong(),
                );
                if ui.small_button("Change files").clicked() {
                    self.page = Page::Files;
                }
            });
            for path in self.inputs.iter().take(3) {
                ui.label(path.file_name().unwrap_or_default().to_string_lossy());
            }
            if self.inputs.len() > 3 {
                ui.weak(format!("And {} more", self.inputs.len() - 3));
            }
            ui.weak("RADD will detect the processor type automatically.");
            ui.add_space(8.0);
            ui.separator();
            eyebrow(ui, "WHERE YOUR RESULTS WILL GO");
            ui.label(
                RichText::new("A separate folder for every analysis")
                    .size(20.0)
                    .strong(),
            );
            path_label(ui, &self.settings.output, "Choose a results folder");
            if ui.button("Change results folder…").clicked() {
                if let Some(path) = rfd::FileDialog::new()
                    .set_title("Where should RADD save your results?")
                    .pick_folder()
                {
                    self.settings.output = path;
                }
            }
            ui.weak("Previous results are kept. Your original files are never overwritten.");
        });
        ui.add_space(8.0);
        card(MINT).show(ui, |ui| {
            eyebrow(ui,"INCLUDED IN EVERY ANALYSIS");
            ui.label(RichText::new("Readable instructions").size(21.0).strong());
            ui.label("An assembly file (.asm) shows addresses and instructions. We'll help you read it on the results screen.");
        });
        ui.add_space(4.0);
        egui::CollapsingHeader::new("Optional export formats").show(ui, |ui| {
            ui.weak("You can skip these for your first look. They are useful when working with other tools.");
            ui.checkbox(&mut self.settings.json,"JSON — structured data for other software");
            ui.checkbox(&mut self.settings.fasta,"FASTA — instruction sequences for comparison");
            ui.checkbox(&mut self.settings.frequency,"Frequency JSON — instruction frequency data");
        });
        if self.config.is_some()
            || self.settings.disable_linear_sweep
            || self.settings.disable_pattern_search
            || self.settings.disable_symbol_parsing
            || self.settings.disable_fs_parsing
            || self.settings.disable_got_parsing
            || self.settings.disable_opd_parsing
        {
            note(
                ui,
                "Custom analysis settings are active. Review them in Engine & settings if you did not intend to change the defaults.",
            );
        }
    }

    fn results_page(&mut self, ui: &mut egui::Ui) {
        let (title, subtitle) = if self.job.is_some() {
            (
                "Looking inside your file…",
                "You can leave this window open while RADD works. Larger programs take longer.",
            )
        } else if self.run_error {
            (
                "This run needs your attention.",
                "Your original files are unchanged. The analysis log below can help explain what happened.",
            )
        } else if self.engine_check {
            (
                "Your engine check is complete.",
                "The log below lists the processor types your engine can read. Choose a file to begin an analysis.",
            )
        } else {
            (
                "Your results are ready to explore.",
                "Start with the readable instructions below. You can open the results folder to access the complete files.",
            )
        };
        heading(ui, "STEP 03 / EXPLORE RESULTS", title, subtitle);
        if let Some(job) = &self.job {
            card(MINT).show(ui, |ui| {
                ui.horizontal(|ui| { ui.spinner(); ui.label(format!("Working for {} seconds",job.started.elapsed().as_secs())); });
                ui.label("RADD does not provide a percentage. New messages appear in the log as work progresses.");
            });
        } else if !self.run_error && !self.engine_check {
            card(MINT).show(ui, |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.label(
                        RichText::new(format!("{} output files saved", self.files.len()))
                            .strong()
                            .color(GREEN),
                    );
                    if ui.button("Open results folder").clicked() {
                        if let Some(dir) = self.directory.clone() {
                            self.report(runner::open_folder(&dir));
                        }
                    }
                });
            });
        }
        if self.job.is_none() && !self.engine_check && !self.files.is_empty() {
            ui.add_space(8.0);
            note(
                ui,
                "Address = where it is. Instruction = what it does. Operands = what it uses.",
            );
            egui::CollapsingHeader::new("Help me read the instructions").default_open(false).show(ui, |ui| {
                ui.columns(3, |cols| {
                    tip(&mut cols[0],"1. Address","A position inside the program. Use an address to locate an instruction.");
                    tip(&mut cols[1],"2. Instruction","A short operation name, such as mov (copy) or call (call a function).");
                    tip(&mut cols[2],"3. Operands","The values or locations an instruction uses. Together, these describe its action.");
                });
                ui.weak("These are machine instructions, not the original source code. You don't need to understand every line on your first visit.");
            });
        }
        ui.add_space(8.0);
        card(Color32::WHITE).show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.selectable_value(&mut self.result_tab,0,"File preview");
                ui.selectable_value(&mut self.result_tab,1,"Analysis log");
            });
            if self.result_tab==0 {
                if self.files.is_empty() { note(ui,"No files to preview yet. Switch to Analysis log for details."); return; }
                let old=self.selected.clone();
                egui::ComboBox::from_id_salt("result-file").width(ui.available_width().min(500.0)).selected_text(self.selected.as_ref().map(|p|output_name(p)).unwrap_or_default()).show_ui(ui, |ui| {
                    for p in &self.files { ui.selectable_value(&mut self.selected,Some(p.clone()),output_name(p)); }
                });
                if old!=self.selected { if let Some(p)=&self.selected { self.preview=runner::read_preview(p,false); self.search.clear(); } }
                if let Some(p)=&self.selected { ui.weak(output_explanation(p)); }
                ui.add(egui::TextEdit::singleline(&mut self.search).hint_text("Find an address or word in this preview…").desired_width(f32::INFINITY));
                ui.weak("Showing up to 256 KB. Search filters only this preview; saved files contain the full output.");
            } else { ui.weak("The latest messages from RADD. The full engine.log is saved in your results folder."); }
            let raw=if self.result_tab==0 { &self.preview } else { &self.log };
            let query=self.search.to_lowercase();
            let lines:Vec<&str>=raw.lines().filter(|line|self.result_tab!=0 || query.is_empty() || line.to_lowercase().contains(&query)).collect();
            egui::Frame::new().fill(INK).corner_radius(10).inner_margin(14).show(ui, |ui| {
                ui.set_min_width(ui.available_width());
                egui::ScrollArea::both().id_salt(("preview",self.result_tab)).max_height(350.0).min_scrolled_height(160.0).auto_shrink([false,false]).show_rows(ui,19.0,lines.len(),|ui,range| {
                    for i in range { ui.add(egui::Label::new(RichText::new(lines[i]).monospace().size(13.0).color(Color32::from_rgb(216,238,226))).wrap_mode(egui::TextWrapMode::Extend)); }
                });
            });
        });
    }

    fn guide_page(&mut self, ui: &mut egui::Ui) {
        heading(
            ui,
            "A LITTLE HELP ALONG THE WAY",
            "You don't need to be an expert.",
            "Start with the sample, follow the three steps, and explore one instruction at a time.",
        );
        card(MINT).show(ui, |ui| {
            ui.label(RichText::new("Your first analysis, in three steps").size(23.0).strong());
            for (n,title,text) in [
                ("01","Choose a file","Use the included sample or browse for a compiled program, such as an .exe or .dll."),
                ("02","Review & analyze","Keep the recommended options, choose a results folder, then select Analyze my files."),
                ("03","Explore results","Start with Readable instructions. The on-screen explanation introduces addresses, instructions and operands."),
            ] {
                ui.separator();
                ui.horizontal(|ui| { ui.label(RichText::new(n).monospace().color(GREEN)); ui.vertical(|ui| { ui.label(RichText::new(title).strong()); ui.label(text); }); });
            }
            if ui.button("Back to choosing a file").clicked() { self.page=Page::Files; }
        });
        ui.add_space(8.0);
        for (title, text) in [
            (
                "What can I open?",
                "A compiled program or library. Windows .exe and .dll files are common examples. Linux ELF files and macOS Mach-O files are supported too. Photos, documents and source-code files are not compiled programs. Raw .bin files may need an expert header configuration.",
            ),
            (
                "What is an engine? Do I need to install one?",
                "The engine is the program that performs the analysis. The Windows download includes it: keep radd.exe beside radd-desktop.exe. The sidebar says Engine located when the file is found. If it is missing, extract the whole download again, or use Engine & settings to locate it.",
            ),
            (
                "What do the assembly columns mean?",
                "From left to right: address; overlap marker (* means overlapping instructions); provenance (how the address was found); instruction bytes; FASTA character; instruction name; and operands. E = entry point, H = header, L = linear sweep. Lowercase e/h indicate recursive traversal. These are clues used by the disassembler, not a guarantee that all displayed bytes are executable code.",
            ),
            (
                "Where are my results?",
                "Review & analyze shows the save location. Each run creates a new subfolder containing outputs, engine.log, run.json (the run settings) and status.txt. Open results folder takes you there. The preview is shortened for speed; files on disk are complete.",
            ),
            (
                "Something went wrong. What should I do?",
                "Read the message shown on screen, then open Analysis log. A missing engine needs setup. A folder permission error means you should choose a writable folder. Unsupported data may need a different input or a custom RHP header configuration. Cancelled or failed runs can contain partial output; check status.txt before using it.",
            ),
            (
                "Which export format should I choose?",
                "Assembly (.asm) is always saved for you to read. JSON holds structured information for other tools. FASTA represents instructions as sequences for comparison; frequency JSON records frequency data. You can ignore the extra formats while learning.",
            ),
        ] {
            card(Color32::WHITE).show(ui, |ui| {
                egui::CollapsingHeader::new(RichText::new(title).strong()).show(ui, |ui| {
                    ui.label(text);
                });
            });
        }
        ui.add_space(10.0);
        ui.hyperlink_to(
            "Visit the PNNL RADD project",
            "https://github.com/pnnl/radd",
        );
        ui.weak("Independent desktop interface. RADD disassembles programs; it does not recover their original source code or act as a debugger.");
    }

    fn settings_window(&mut self, ctx: &egui::Context) {
        let mut open = self.settings_open;
        egui::Window::new("Engine & settings").open(&mut open).default_width(610.0).max_height(650.0).resizable(true).collapsible(false).show(ctx,|ui| {
            egui::ScrollArea::vertical().show(ui,|ui| {
                ui.add_enabled_ui(self.job.is_none(),|ui| {
                    ui.label(RichText::new("The engine does the reading.").size(23.0).strong());
                    ui.label("It is included in the Windows bundle. You normally only need to choose it once.");
                    path_label(ui,&self.settings.engine,"No engine found yet");
                    ui.horizontal_wrapped(|ui| {
                        if ui.button("Locate engine…").clicked() { if let Some(path)=rfd::FileDialog::new().set_title("Select radd.exe on Windows, or radd on Linux/macOS").pick_file() { self.settings.engine=path; } }
                        if ui.button("Find automatically").clicked() { if let Some(p)=runner::find_engine() {self.settings.engine=p;} else {self.report(Err("Engine not found. Keep both executables together, or select the engine with Locate engine.".into()));} }
                        if ui.add_enabled(self.settings.engine.is_file(),egui::Button::new("Check engine")).clicked() { self.launch(true); }
                    });
                    ui.add_space(12.0); ui.separator();
                    egui::CollapsingHeader::new("Advanced analysis options").show(ui,|ui| {
                        note(ui,"For your first analysis, leave these unchanged.");
                        ui.checkbox(&mut self.settings.disable_linear_sweep,"Disable linear sweep").on_hover_text("Skip gap-filling, which can sometimes interpret data as instructions.");
                        ui.checkbox(&mut self.settings.disable_pattern_search,"Disable function-pattern search");
                        ui.checkbox(&mut self.settings.disable_symbol_parsing,"Ignore symbol tables");
                        ui.checkbox(&mut self.settings.disable_fs_parsing,"Ignore Mach-O function-start metadata");
                        ui.checkbox(&mut self.settings.disable_got_parsing,"Ignore ELF .got metadata");
                        ui.checkbox(&mut self.settings.disable_opd_parsing,"Ignore ELF .opd metadata");
                        ui.horizontal(|ui| { ui.label("FASTA prefix"); ui.text_edit_singleline(&mut self.settings.prefix); });
                        ui.checkbox(&mut self.settings.function_names,"Use function names in FASTA (may repeat)");
                        ui.separator();
                        ui.label("Optional RHP header configuration (one input only)");
                        ui.horizontal_wrapped(|ui| {
                            if ui.button("Choose configuration JSON…").clicked() { if let Some(path)=rfd::FileDialog::new().add_filter("JSON",&["json"]).pick_file() {self.config=Some(path);} }
                            if ui.button("Clear configuration").clicked() {self.config=None;}
                        });
                        if let Some(path)=&self.config {path_label(ui,path,"");}
                        ui.weak("This describes a raw or unsupported file's header. It is different from the JSON that RADD produces.");
                        if ui.button("Restore recommended analysis settings").clicked() {
                            let engine=self.settings.engine.clone(); let output=self.settings.output.clone();
                            self.settings=Settings {engine,output,..Settings::default()}; self.config=None;
                        }
                    });
                });
            });
        });
        self.settings_open = open && !(self.engine_check && self.job.is_some());
    }

    fn footer(&mut self, ui: &mut egui::Ui) {
        if !self.message.is_empty() && self.is_error {
            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new(&self.message).color(Color32::from_rgb(153, 48, 38)));
                if ui.small_button("Dismiss").clicked() {
                    self.message.clear();
                }
            });
            ui.separator();
        }
        ui.horizontal(|ui| {
            if let Some(job) = &self.job {
                ui.spinner();
                ui.label("Analysis in progress");
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button("Cancel analysis").clicked() {
                        job.cancel.store(true, Ordering::Relaxed);
                    }
                });
                return;
            }
            match self.page {
                Page::Files => {
                    ui.weak(if self.inputs.is_empty() {
                        "Start with the sample or choose a file."
                    } else {
                        "File selected. Let's choose where to save."
                    });
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if primary(ui, "Continue to review", !self.inputs.is_empty()).clicked() {
                            self.page = Page::Review;
                            self.message.clear();
                            self.is_error = false;
                        }
                    });
                }
                Page::Review => {
                    if ui.button("Back").clicked() {
                        self.page = Page::Files;
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if primary(
                            ui,
                            "Analyze my files",
                            !self.inputs.is_empty() && self.settings.engine.is_file(),
                        )
                        .on_hover_text("Creates a new results folder and starts RADD.")
                        .clicked()
                        {
                            self.launch(false);
                        }
                    });
                }
                Page::Results => {
                    ui.weak(if self.run_error {
                        "You can adjust the settings and try again."
                    } else {
                        "Your original files are unchanged."
                    });
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if primary(ui, "Choose another file", true).clicked() {
                            self.page = Page::Files;
                            self.inputs.clear();
                            self.message.clear();
                        }
                    });
                }
                Page::Guide => {
                    ui.weak("Take it one step at a time. The sample is a good place to start.");
                }
            }
        });
    }
}

struct Card(Color32);
fn card(fill: Color32) -> Card {
    Card(fill)
}
impl Card {
    fn show<R>(
        &self,
        ui: &mut egui::Ui,
        contents: impl FnOnce(&mut egui::Ui) -> R,
    ) -> egui::InnerResponse<R> {
        egui::Frame::new()
            .fill(self.0)
            .stroke(Stroke::new(1.0, LINE))
            .corner_radius(14)
            .inner_margin(22)
            .show(ui, |ui| {
                ui.set_min_width(ui.available_width());
                contents(ui)
            })
    }
}
fn eyebrow(ui: &mut egui::Ui, text: &str) {
    ui.label(RichText::new(text).size(11.0).strong().color(GREEN));
}
fn heading(ui: &mut egui::Ui, kicker: &str, title: &str, subtitle: &str) {
    eyebrow(ui, kicker);
    ui.add_space(2.0);
    ui.label(RichText::new(title).size(32.0).strong().color(INK));
    ui.label(RichText::new(subtitle).size(16.0).color(MUTED));
    ui.add_space(16.0);
}
fn primary(ui: &mut egui::Ui, text: &str, enabled: bool) -> egui::Response {
    ui.add_enabled(
        enabled,
        egui::Button::new(RichText::new(text).strong().color(Color32::WHITE))
            .fill(GREEN)
            .corner_radius(8)
            .min_size(egui::vec2(0.0, 44.0)),
    )
}
fn note(ui: &mut egui::Ui, text: &str) {
    ui.label(RichText::new(text).size(14.0).color(MUTED));
}
fn tip(ui: &mut egui::Ui, title: &str, text: &str) {
    ui.label(RichText::new(title).size(16.0).strong().color(INK));
    ui.label(RichText::new(text).size(14.0).color(MUTED));
}
fn path_label(ui: &mut egui::Ui, path: &Path, empty: &str) {
    ui.add(
        egui::Label::new(
            RichText::new(if path.as_os_str().is_empty() {
                empty.to_owned()
            } else {
                path.display().to_string()
            })
            .size(14.0)
            .color(MUTED),
        )
        .wrap(),
    );
}
fn file_art(ui: &mut egui::Ui, size: f32) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(size, size + 10.0), egui::Sense::hover());
    let p = ui.painter();
    let sheet = egui::Rect::from_min_size(
        rect.min + egui::vec2(10.0, 2.0),
        egui::vec2(size - 24.0, size),
    );
    p.rect_filled(sheet, 9.0, Color32::WHITE);
    p.rect_stroke(
        sheet,
        9.0,
        Stroke::new(1.0, Color32::from_rgb(150, 192, 172)),
        egui::StrokeKind::Inside,
    );
    for i in 0..3 {
        let x = sheet.left() + 12.0;
        let y = sheet.top() + 19.0 + i as f32 * 12.0;
        p.line_segment(
            [egui::pos2(x, y), egui::pos2(sheet.right() - 12.0, y)],
            Stroke::new(3.0, Color32::from_rgb(145, 188, 167)),
        );
    }
    p.circle_filled(sheet.right_bottom() - egui::vec2(0.0, 5.0), 14.0, GREEN);
    p.text(
        sheet.right_bottom() - egui::vec2(0.0, 5.0),
        egui::Align2::CENTER_CENTER,
        "+",
        egui::FontId::proportional(21.0),
        Color32::WHITE,
    );
}
fn output_name(path: &Path) -> String {
    let name = path.file_name().unwrap_or_default().to_string_lossy();
    let kind = if name.ends_with(".asm") {
        "Readable instructions"
    } else if name.ends_with(".freq.json") {
        "Instruction frequencies"
    } else if name.ends_with(".json") {
        "Structured data"
    } else {
        "Instruction sequences"
    };
    format!("{kind}  ·  {name}")
}
fn output_explanation(path: &Path) -> &'static str {
    let name = path.file_name().unwrap_or_default().to_string_lossy();
    if name.ends_with(".asm") {
        "Assembly: addresses, instruction bytes, operation names and operands."
    } else if name.ends_with(".freq.json") {
        "Frequency data for comparing how often instruction categories occur."
    } else if name.ends_with(".json") {
        "JSON: information organized into fields for other software to read."
    } else {
        "FASTA: instructions represented as character sequences for comparison."
    }
}

impl eframe::App for Desktop {
    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        #[cfg(feature = "ui-capture")]
        if self.capture.is_some() {
            return;
        }
        eframe::set_value(storage, "settings-v1", &self.settings);
    }
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.poll(ctx);
        #[cfg(feature = "ui-capture")]
        self.capture_tick(ctx);
        if self.job.is_none() {
            let paths: Vec<_> = ctx.input(|i| {
                i.raw
                    .dropped_files
                    .iter()
                    .filter_map(|f| f.path.clone())
                    .collect()
            });
            if !paths.is_empty() {
                self.add_inputs(paths);
            }
        }
        egui::SidePanel::left("navigation-v2")
            .exact_width(234.0)
            .resizable(false)
            .frame(egui::Frame::new().fill(INK).inner_margin(18))
            .show(ctx, |ui| self.sidebar(ui));
        egui::TopBottomPanel::bottom("actions-v2")
            .frame(
                egui::Frame::new()
                    .fill(Color32::WHITE)
                    .inner_margin(egui::Margin::symmetric(24, 16))
                    .stroke(Stroke::new(1.0, LINE)),
            )
            .show(ctx, |ui| self.footer(ui));
        egui::CentralPanel::default()
            .frame(
                egui::Frame::new()
                    .fill(PAPER)
                    .inner_margin(egui::Margin::symmetric(30, 28)),
            )
            .show(ctx, |ui| {
                egui::ScrollArea::vertical()
                    .id_salt(match self.page {
                        Page::Files => "files-v2",
                        Page::Review => "review-v2",
                        Page::Results => "results-v2",
                        Page::Guide => "guide-v2",
                    })
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        ui.set_min_width(ui.available_width());
                        match self.page {
                            Page::Files => {
                                ui.add_enabled_ui(self.job.is_none(), |ui| self.files_page(ui));
                            }
                            Page::Review => {
                                ui.add_enabled_ui(self.job.is_none(), |ui| self.review_page(ui));
                            }
                            Page::Results => self.results_page(ui),
                            Page::Guide => self.guide_page(ui),
                        }
                    });
            });
        if self.settings_open {
            self.settings_window(ctx);
        }
    }
}

// Optional app-internal visual smoke test. Never enabled in the shipped build.
#[cfg(feature = "ui-capture")]
struct Capture {
    folder: PathBuf,
    index: usize,
    frames: usize,
    waiting: bool,
    stage_started: Instant,
}
#[cfg(feature = "ui-capture")]
impl Desktop {
    fn with_capture(mut self) -> Self {
        if let Some(folder) = std::env::var_os("RADD_UI_CAPTURE_DIR") {
            let folder = PathBuf::from(folder);
            std::fs::create_dir_all(&folder).unwrap();
            self.settings = Settings {
                engine: PathBuf::from(
                    std::env::var_os("RADD_TEST_ENGINE").expect("Set RADD_TEST_ENGINE"),
                ),
                output: folder.join("analysis"),
                ..Settings::default()
            };
            self.capture = Some(Capture {
                folder,
                index: 0,
                frames: 0,
                waiting: false,
                stage_started: Instant::now(),
            });
        }
        self
    }
    fn capture_tick(&mut self, ctx: &egui::Context) {
        let Some(mut cap) = self.capture.take() else {
            return;
        };
        ctx.request_repaint_after(Duration::from_millis(80));
        let screenshot = ctx.input(|i| {
            i.events.iter().find_map(|e| {
                if let egui::Event::Screenshot { image, .. } = e {
                    Some(image.clone())
                } else {
                    None
                }
            })
        });
        if let Some(image) = screenshot {
            let bytes: Vec<u8> = image.pixels.iter().flat_map(|p| p.to_array()).collect();
            image::save_buffer(
                cap.folder.join(format!("{:02}.png", cap.index)),
                &bytes,
                image.width() as u32,
                image.height() as u32,
                image::ColorType::Rgba8,
            )
            .unwrap();
            cap.index += 1;
            cap.frames = 0;
            cap.waiting = false;
            cap.stage_started = Instant::now();
            match cap.index {
                1 => self.load_sample(),
                2 => self.page = Page::Review,
                3 => self.launch(false),
                4 => self.page = Page::Guide,
                5 => {
                    self.page = Page::Files;
                    self.settings_open = true;
                }
                6 => {
                    self.settings_open = false;
                    self.settings.engine = PathBuf::new();
                    self.page = Page::Review;
                }
                7 => {
                    self.settings.engine =
                        PathBuf::from(std::env::var_os("RADD_TEST_ENGINE").unwrap());
                    let input = cap.folder.join("unsupported.bin");
                    std::fs::write(&input, b"not a compiled program").unwrap();
                    self.inputs = vec![input];
                    self.launch(false);
                }
                _ => {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
            }
        }
        cap.frames += 1;
        if cap.index < 8
            && cap.frames > 5
            && cap.stage_started.elapsed() > Duration::from_millis(400)
            && !cap.waiting
            && self.job.is_none()
        {
            ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(egui::UserData::default()));
            cap.waiting = true;
        }
        self.capture = Some(cap);
    }
}
