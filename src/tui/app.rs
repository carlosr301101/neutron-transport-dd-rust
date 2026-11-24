//! TUI application state, input handling and rendering.

use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;
use std::time::Duration;

use ratatui::Frame;
use ratatui::crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind};
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Cell, Clear, Paragraph, Row, Table, TableState, Tabs, Wrap};

use serde_json::Value;

use crate::config::Config;
use crate::paths;
use crate::post::{excel, extract, plot as plot_post};
use crate::sweep::{self, SweepRange};
use crate::tui::model::{ConfigModel, FieldKind, Validation};
use crate::tui::services::{
    config_store, results as results_service,
    runner::{self, RunEvent, RunHandle},
};

/// Tabs shown in the title bar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tab {
    /// Configuration editor.
    Config,
    /// Run launcher and live log.
    Run,
    /// Result browser.
    Results,
    /// Plot generation.
    Plots,
    /// Sweep placeholders.
    Placeholders,
    /// Dataset and Excel export.
    Export,
}

impl Tab {
    /// Every tab in display order.
    pub const ALL: [Tab; 6] = [
        Tab::Config,
        Tab::Run,
        Tab::Results,
        Tab::Plots,
        Tab::Placeholders,
        Tab::Export,
    ];

    /// Title shown in the tab bar.
    pub const fn title(self) -> &'static str {
        match self {
            Tab::Config => "Config",
            Tab::Run => "Run",
            Tab::Results => "Results",
            Tab::Plots => "Plots",
            Tab::Placeholders => "Placeholders",
            Tab::Export => "Export",
        }
    }

    fn index(self) -> usize {
        Tab::ALL.iter().position(|tab| *tab == self).unwrap_or(0)
    }

    fn next(self) -> Tab {
        Tab::ALL[(self.index() + 1) % Tab::ALL.len()]
    }

    fn previous(self) -> Tab {
        Tab::ALL[(self.index() + Tab::ALL.len() - 1) % Tab::ALL.len()]
    }
}

/// Severity of the status message.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Level {
    /// Informational message.
    Info,
    /// Success message.
    Ok,
    /// Error message.
    Error,
}

/// Target of an in-progress edit.
#[derive(Debug, Clone, Copy)]
enum EditTarget {
    Field(FieldKind),
    Sweep { row: usize, column: usize },
}

/// In-progress cell edit.
#[derive(Debug, Clone)]
pub struct Edit {
    target: EditTarget,
    label: String,
    buffer: String,
}

/// Placeholder ranges being edited.
#[derive(Default)]
pub struct SweepState {
    /// Ranges.
    pub ranges: Vec<SweepRange>,
    /// Selected range.
    pub selected: usize,
    /// Selected column (0 name, 1 start, 2 end, 3 step).
    pub column: usize,
    /// Last expansion message.
    pub message: String,
}

/// Plot generation state.
#[derive(Default)]
pub struct PlotState {
    /// Index into `PLOT_KINDS`.
    pub kind: usize,
    /// Last generated file.
    pub last: Option<PathBuf>,
    /// Last message.
    pub message: String,
    /// Inline preview (feature `images`).
    #[cfg(feature = "images")]
    pub preview: Option<ratatui_image::protocol::StatefulProtocol>,
}

/// Export state.
#[derive(Default)]
pub struct ExportState {
    /// Last message.
    pub message: String,
    /// Last exported file.
    pub last: Option<PathBuf>,
}

/// Plot kinds offered by the Plots screen.
pub const PLOT_KINDS: [&str; 4] = ["scalar", "angular", "k-history", "scs"];

/// State of the run screen.
#[derive(Default)]
pub struct RunState {
    /// Configurations found in `outputs/inputs`.
    pub entries: Vec<config_store::ConfigEntry>,
    /// Selected configuration.
    pub selected: usize,
    /// Active background run.
    pub handle: Option<RunHandle>,
    /// Log lines.
    pub log: Vec<String>,
    /// Whether a run is in progress.
    pub running: bool,
    /// Output path of the last run.
    pub output: Option<PathBuf>,
    /// Name of the current/last run.
    pub current: Option<String>,
}

/// State of the results screen.
#[derive(Default)]
pub struct ResultsState {
    /// Output files found in `outputs/results`.
    pub entries: Vec<results_service::OutputEntry>,
    /// Selected output.
    pub selected: usize,
    /// Loaded payload.
    pub payload: Option<Value>,
}

/// Application state.
pub struct App {
    /// Active tab.
    pub tab: Tab,
    /// Configuration being edited.
    pub model: ConfigModel,
    /// Selected row of the form.
    pub selected: usize,
    /// Persistent table state (keeps the scroll offset).
    pub table_state: TableState,
    /// Active edit, if any.
    pub editing: Option<Edit>,
    /// Last validation result.
    pub validation: Validation,
    /// Status line.
    pub status: String,
    /// Status severity.
    pub level: Level,
    /// Set to true to exit.
    pub quit: bool,
    /// Run screen state.
    pub run: RunState,
    /// Results screen state.
    pub results: ResultsState,
    /// Placeholders screen state.
    pub sweeps: SweepState,
    /// Plots screen state.
    pub plots: PlotState,
    /// Export screen state.
    pub export: ExportState,
}

impl Default for App {
    fn default() -> Self {
        let model = ConfigModel::default();
        let validation = model.validate();
        let run = RunState {
            entries: config_store::list(),
            ..RunState::default()
        };
        let results = ResultsState {
            entries: results_service::list(),
            ..ResultsState::default()
        };
        App {
            tab: Tab::Config,
            model,
            selected: 0,
            table_state: TableState::default().with_selected(Some(0)),
            editing: None,
            validation,
            status: "Ready. Press ? or see the footer for keys.".into(),
            level: Level::Info,
            quit: false,
            run,
            results,
            sweeps: SweepState::default(),
            plots: PlotState::default(),
            export: ExportState::default(),
        }
    }
}

impl App {
    /// Run the event loop until the user quits.
    ///
    /// # Errors
    ///
    /// Propagates terminal I/O errors.
    pub fn run(&mut self, terminal: &mut ratatui::DefaultTerminal) -> std::io::Result<()> {
        loop {
            terminal.draw(|frame| self.render(frame))?;
            if event::poll(Duration::from_millis(200))? {
                match event::read()? {
                    Event::Key(key) if key.kind == KeyEventKind::Press => {
                        self.handle_key(key);
                    }
                    _ => {}
                }
            }
            self.poll_runner();
            if self.quit {
                break;
            }
        }
        Ok(())
    }

    /// Handle one key press.
    pub fn handle_key(&mut self, key: KeyEvent) {
        if key.kind != KeyEventKind::Press {
            return;
        }

        if let Some(edit) = self.editing.as_mut() {
            match key.code {
                KeyCode::Esc => self.editing = None,
                KeyCode::Enter => {
                    let edit = self.editing.take().expect("edit");
                    self.apply_edit(edit);
                }
                KeyCode::Backspace => {
                    edit.buffer.pop();
                }
                KeyCode::Char(character) => edit.buffer.push(character),
                _ => {}
            }
            return;
        }

        match key.code {
            KeyCode::Char('q') => {
                self.quit = true;
                return;
            }
            KeyCode::Tab => {
                self.tab = self.tab.next();
                self.on_tab_enter();
                return;
            }
            KeyCode::BackTab => {
                self.tab = self.tab.previous();
                self.on_tab_enter();
                return;
            }
            _ => {}
        }

        match self.tab {
            Tab::Config => match key.code {
                KeyCode::Up => self.move_selection(-1),
                KeyCode::Down => self.move_selection(1),
                KeyCode::Left => self.cycle(-1),
                KeyCode::Right => self.cycle(1),
                KeyCode::Enter => self.activate(),
                KeyCode::Char('s') => self.save(),
                KeyCode::Char('v') => self.revalidate(),
                _ => {}
            },
            Tab::Run => match key.code {
                KeyCode::Up => self.move_run_selection(-1),
                KeyCode::Down => self.move_run_selection(1),
                KeyCode::Char('r') => self.start_run(),
                KeyCode::Char('c') => self.cancel_run(),
                _ => {}
            },
            Tab::Results => match key.code {
                KeyCode::Up => self.move_results_selection(-1),
                KeyCode::Down => self.move_results_selection(1),
                _ => {}
            },
            Tab::Placeholders => match key.code {
                KeyCode::Up => self.move_sweep_selection(-1),
                KeyCode::Down => self.move_sweep_selection(1),
                KeyCode::Left => self.sweeps.column = self.sweeps.column.saturating_sub(1),
                KeyCode::Right => self.sweeps.column = (self.sweeps.column + 1).min(3),
                KeyCode::Char('a') => self.add_sweep_range(),
                KeyCode::Char('d') => self.remove_sweep_range(),
                KeyCode::Enter => self.edit_sweep_cell(),
                KeyCode::Char('x') => self.expand_placeholders(),
                _ => {}
            },
            Tab::Plots => match key.code {
                KeyCode::Char('k') | KeyCode::Right => {
                    self.plots.kind = (self.plots.kind + 1) % PLOT_KINDS.len();
                }
                KeyCode::Left => {
                    self.plots.kind = (self.plots.kind + PLOT_KINDS.len() - 1) % PLOT_KINDS.len();
                }
                KeyCode::Char('g') => self.generate_plot(),
                KeyCode::Char('o') => self.open_last_plot(),
                _ => {}
            },
            Tab::Export => match key.code {
                KeyCode::Char('d') => self.export_data(),
                KeyCode::Char('x') => self.export_excel(),
                KeyCode::Char('s') => self.export_scs(),
                KeyCode::Char('o') => self.open_last_export(),
                _ => {}
            },
        }
    }

    /// Save the configuration to the next auto-numbered inputs path.
    pub fn save(&mut self) {
        if let Validation::Invalid(error) = self.model.validate() {
            self.validation = Validation::Invalid(error.clone());
            self.status = format!("Cannot save: {error}");
            self.level = Level::Error;
            return;
        }
        let path = paths::next_input_path();
        match self.save_to(&path) {
            Ok(()) => {
                self.status = format!("Saved {}", path.display());
                self.level = Level::Ok;
            }
            Err(error) => {
                self.status = format!("Save failed: {error}");
                self.level = Level::Error;
            }
        }
    }

    /// Save the configuration to an explicit path.
    ///
    /// # Errors
    ///
    /// Propagates filesystem or serialization errors.
    pub fn save_to(&mut self, path: &Path) -> Result<(), String> {
        let value = self.model.to_value();
        crate::cli::write_json(path, &value)?;
        self.validation = self.model.validate();
        Ok(())
    }

    /// Re-run validation and refresh the status line.
    pub fn revalidate(&mut self) {
        self.validation = self.model.validate();
        self.status = match &self.validation {
            Validation::Valid(config) => format!(
                "Valid configuration: {} cells, mode {}.",
                config.ntc,
                config.mode.as_str()
            ),
            Validation::Template(names) => {
                format!("Template with placeholders: {}.", names.join(", "))
            }
            Validation::Invalid(error) => format!("Invalid: {error}"),
        };
        self.level = match self.validation {
            Validation::Valid(_) => Level::Ok,
            _ => Level::Info,
        };
    }

    fn on_tab_enter(&mut self) {
        match self.tab {
            Tab::Run => self.refresh_run_entries(),
            Tab::Results => self.refresh_results(),
            _ => {}
        }
    }

    /// Reload the configuration list from `outputs/inputs`.
    pub fn refresh_run_entries(&mut self) {
        self.run.entries = config_store::list();
        if self.run.selected >= self.run.entries.len() {
            self.run.selected = self.run.entries.len().saturating_sub(1);
        }
    }

    /// Reload the output list from `outputs/results` and read the selection.
    pub fn refresh_results(&mut self) {
        self.results.entries = results_service::list();
        if self.results.selected >= self.results.entries.len() {
            self.results.selected = self.results.entries.len().saturating_sub(1);
        }
        self.load_selected_result();
    }

    fn load_selected_result(&mut self) {
        let Some(entry) = self.results.entries.get(self.results.selected) else {
            self.results.payload = None;
            return;
        };
        match results_service::load(&entry.path) {
            Ok(payload) => self.results.payload = Some(payload),
            Err(error) => {
                self.results.payload = None;
                self.status = format!("Could not read {}: {error}", entry.name);
                self.level = Level::Error;
            }
        }
    }

    /// Load a result payload from an explicit path.
    ///
    /// # Errors
    ///
    /// Propagates read/parse errors.
    pub fn load_results_from(&mut self, path: &Path) -> Result<(), String> {
        self.results.payload = Some(results_service::load(path)?);
        Ok(())
    }

    fn move_run_selection(&mut self, delta: i32) {
        let count = self.run.entries.len();
        if count == 0 {
            return;
        }
        self.run.selected = (self.run.selected as i32 + delta).clamp(0, count as i32 - 1) as usize;
    }

    fn move_results_selection(&mut self, delta: i32) {
        let count = self.results.entries.len();
        if count == 0 {
            return;
        }
        self.results.selected =
            (self.results.selected as i32 + delta).clamp(0, count as i32 - 1) as usize;
        self.load_selected_result();
    }

    /// Run the selected configuration to the next auto-numbered output.
    pub fn start_run(&mut self) {
        self.start_run_with_output(paths::next_output_path());
    }

    /// Run the selected configuration to an explicit output path.
    pub fn start_run_with_output(&mut self, output: PathBuf) {
        if self.run.running {
            self.status = "A run is already in progress.".into();
            self.level = Level::Info;
            return;
        }
        let Some(entry) = self.run.entries.get(self.run.selected).cloned() else {
            self.status = "No configuration selected (outputs/inputs is empty).".into();
            self.level = Level::Error;
            return;
        };
        let value = match config_store::load(&entry.path) {
            Ok(value) => value,
            Err(error) => {
                self.status = format!("Could not read {}: {error}", entry.name);
                self.level = Level::Error;
                return;
            }
        };
        let config = match Config::from_value(&value, 1e-5, 2000, None) {
            Ok(config) => config,
            Err(error) => {
                self.status = format!("{}: {error}", entry.name);
                self.level = Level::Error;
                return;
            }
        };

        let name = entry.name.clone();
        self.run.log.clear();
        self.run
            .log
            .push(format!("config: {}", entry.path.display()));
        self.run.current = Some(name.clone());
        self.run.output = Some(output.clone());
        self.run.running = true;
        self.run.handle = Some(runner::spawn(
            name,
            value,
            config,
            None,
            output,
            Some(entry.path.to_string_lossy().to_string()),
        ));
        self.status = "Run started.".into();
        self.level = Level::Info;
    }

    /// Request cancellation of the active run.
    pub fn cancel_run(&mut self) {
        if let Some(handle) = &self.run.handle {
            handle.cancel.store(true, Ordering::Relaxed);
            self.run.log.push("cancellation requested".into());
            self.status = "Cancelling…".into();
            self.level = Level::Info;
        }
    }

    /// Drain background events. Call once per UI tick.
    pub fn poll_runner(&mut self) {
        let events: Vec<RunEvent> = match self.run.handle.as_ref() {
            Some(handle) => handle.receiver.try_iter().collect(),
            None => Vec::new(),
        };
        for event in events {
            match event {
                RunEvent::Log(line) => self.run.log.push(line),
                RunEvent::Done { result, output } => {
                    self.run.running = false;
                    self.run.handle = None;
                    self.run.log.push(format!(
                        "done: converged={} iteration={} k_eff={:?}",
                        result.converged, result.iteration, result.k_eff
                    ));
                    self.run.output = Some(output.clone());
                    self.status = format!("Run finished: {}", output.display());
                    self.level = Level::Ok;
                    self.refresh_results();
                }
                RunEvent::Failed(error) => {
                    self.run.running = false;
                    self.run.handle = None;
                    self.run.log.push(format!("error: {error}"));
                    self.status = format!("Run failed: {error}");
                    self.level = Level::Error;
                }
            }
        }
        if self.run.log.len() > 500 {
            let excess = self.run.log.len() - 500;
            self.run.log.drain(0..excess);
        }
    }

    fn move_sweep_selection(&mut self, delta: i32) {
        let count = self.sweeps.ranges.len();
        if count == 0 {
            return;
        }
        self.sweeps.selected =
            (self.sweeps.selected as i32 + delta).clamp(0, count as i32 - 1) as usize;
    }

    fn add_sweep_range(&mut self) {
        let used: Vec<&str> = self
            .sweeps
            .ranges
            .iter()
            .map(|range| range.name.as_str())
            .collect();
        let name = ["kk", "mm", "tt", "pp"]
            .iter()
            .find(|name| !used.contains(name))
            .copied()
            .unwrap_or("kk");
        self.sweeps.ranges.push(SweepRange::new(name));
        self.sweeps.selected = self.sweeps.ranges.len() - 1;
        self.sweeps.message.clear();
    }

    fn remove_sweep_range(&mut self) {
        if self.sweeps.ranges.is_empty() {
            return;
        }
        self.sweeps.ranges.remove(self.sweeps.selected);
        self.sweeps.selected = self
            .sweeps
            .selected
            .min(self.sweeps.ranges.len().saturating_sub(1));
    }

    fn edit_sweep_cell(&mut self) {
        if self.sweeps.ranges.is_empty() {
            return;
        }
        self.start_sweep_edit(self.sweeps.selected, self.sweeps.column);
    }

    /// Expand the current ranges over the selected configuration.
    pub fn expand_placeholders(&mut self) {
        let output = paths::outputs_dir().join("inputs").join("inputs_1d.json");
        self.expand_placeholders_to(output);
    }

    /// Expand the current ranges and write the list to an explicit path.
    pub fn expand_placeholders_to(&mut self, output: PathBuf) {
        let Some(entry) = self.run.entries.get(self.run.selected).cloned() else {
            self.sweeps.message = "No configuration selected (outputs/inputs is empty).".into();
            self.level = Level::Error;
            return;
        };
        let value = match config_store::load(&entry.path) {
            Ok(value) => value,
            Err(error) => {
                self.sweeps.message = format!("Could not read {}: {error}", entry.name);
                self.level = Level::Error;
                return;
            }
        };

        match sweep::expand(&[(entry.name.clone(), value)], &self.sweeps.ranges) {
            Ok(expansion) if expansion.configs.is_empty() => {
                self.sweeps.message = format!(
                    "No configurations generated ({} problem(s)).",
                    expansion.errors.len()
                );
                self.level = Level::Error;
            }
            Ok(expansion) => {
                let total = expansion.configs.len();
                match crate::cli::write_json(&output, &Value::Array(expansion.configs)) {
                    Ok(()) => {
                        self.sweeps.message = format!(
                            "Generated {total} config(s) in {}{}",
                            output.display(),
                            if expansion.errors.is_empty() {
                                String::new()
                            } else {
                                format!(" ({} skipped)", expansion.errors.len())
                            }
                        );
                        self.status = self.sweeps.message.clone();
                        self.level = Level::Ok;
                    }
                    Err(error) => {
                        self.sweeps.message = format!("Write failed: {error}");
                        self.level = Level::Error;
                    }
                }
            }
            Err(error) => {
                self.sweeps.message = format!("Expansion failed: {error}");
                self.status = self.sweeps.message.clone();
                self.level = Level::Error;
            }
        }
    }

    fn selected_output(&self) -> Result<(Value, PathBuf), String> {
        let entry = self
            .results
            .entries
            .get(self.results.selected)
            .ok_or_else(|| "No output selected in Results.".to_string())?;
        let payload = results_service::load(&entry.path)?;
        Ok((payload, entry.path.clone()))
    }

    fn build_datos_value(&self, payload: &Value, source: &Path) -> Result<Value, String> {
        let runs = payload
            .get("runs")
            .and_then(Value::as_array)
            .ok_or_else(|| "payload has no runs[]".to_string())?;
        let first = runs
            .first()
            .ok_or_else(|| "payload has no runs".to_string())?;
        let length = first
            .get("scalar_flux")
            .and_then(Value::as_array)
            .map_or(0, Vec::len);
        if length == 0 {
            return Err("first run has no scalar_flux".to_string());
        }
        let positions = if length > 1 {
            vec![0, length - 1]
        } else {
            vec![0]
        };
        let extraction = extract::extract(payload, &source.to_string_lossy(), &positions, None)?;
        Ok(extraction.payload)
    }

    /// Generate the plot for the selected kind into the default path.
    pub fn generate_plot(&mut self) {
        let kind = PLOT_KINDS[self.plots.kind];
        let output = paths::results_dir()
            .join("plots")
            .join(format!("{kind}.png"));
        self.generate_kind_to(self.plots.kind, output);
    }

    /// Generate a plot into an explicit path.
    pub fn generate_plot_to(&mut self, output: PathBuf) {
        self.generate_kind_to(self.plots.kind, output);
    }

    fn generate_kind_to(&mut self, kind_index: usize, output: PathBuf) {
        let kind = PLOT_KINDS[kind_index];
        let result = (|| -> Result<(), String> {
            let (payload, source) = self.selected_output()?;
            let runs = payload
                .get("runs")
                .and_then(Value::as_array)
                .ok_or_else(|| "payload has no runs[]".to_string())?;
            if runs.is_empty() {
                return Err("payload has no runs".to_string());
            }
            let indices: Vec<usize> = (0..runs.len()).collect();
            let size = plot_post::PlotSize::default();

            match kind {
                "scalar" => plot_post::scalar(&payload, &indices, &output, size, false),
                "angular" => plot_post::angular(&payload, indices[0], &output, size, false),
                "k-history" => plot_post::k_history(&payload, &indices, &output, size),
                _ => {
                    let datos = self.build_datos_value(&payload, &source)?;
                    plot_post::scs(&datos, "SCS", &output, size)
                }
            }
        })();

        match result {
            Ok(()) => {
                self.plots.last = Some(output.clone());
                self.plots.message = format!("Saved {}", output.display());
                self.status = self.plots.message.clone();
                self.level = Level::Ok;
                #[cfg(feature = "images")]
                {
                    self.plots.preview = load_preview(&output);
                }
            }
            Err(error) => {
                self.plots.message = format!("Plot failed: {error}");
                self.status = self.plots.message.clone();
                self.level = Level::Error;
            }
        }
    }

    fn open_last_plot(&mut self) {
        if let Some(path) = &self.plots.last {
            crate::cli::plot::open_file(path);
        }
    }

    /// Export `datos.json` into the default results path.
    pub fn export_data(&mut self) {
        self.export_data_to(paths::results_dir().join("datos.json"));
    }

    /// Export `datos.json` into an explicit path.
    pub fn export_data_to(&mut self, path: PathBuf) {
        let result = (|| -> Result<(), String> {
            let (payload, source) = self.selected_output()?;
            let datos = self.build_datos_value(&payload, &source)?;
            crate::cli::write_json(&path, &datos)
        })();
        self.finish_export(result, path, "Data");
    }

    /// Export the workbook next to the selected output.
    pub fn export_excel(&mut self) {
        match self.selected_output() {
            Ok((_, source)) => {
                let path = source.with_extension("xlsx");
                self.export_excel_to(path);
            }
            Err(error) => {
                self.export.message = error.clone();
                self.status = error;
                self.level = Level::Error;
            }
        }
    }

    /// Export the workbook into an explicit path.
    pub fn export_excel_to(&mut self, path: PathBuf) {
        let result = (|| -> Result<(), String> {
            let (payload, _) = self.selected_output()?;
            let runs = payload
                .get("runs")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default();
            excel::write_workbook(&payload, &runs, &path)
        })();
        self.finish_export(result, path, "Excel");
    }

    /// Export the SCS plot into the default plots directory.
    pub fn export_scs(&mut self) {
        self.export_scs_to(paths::results_dir().join("plots").join("scs.png"));
    }

    /// Export the SCS plot into an explicit path.
    pub fn export_scs_to(&mut self, path: PathBuf) {
        self.generate_kind_to(3, path);
        self.export.message = self.plots.message.clone();
        self.export.last = self.plots.last.clone();
    }

    fn finish_export(&mut self, result: Result<(), String>, path: PathBuf, label: &str) {
        match result {
            Ok(()) => {
                self.export.message = format!("{label} saved: {}", path.display());
                self.export.last = Some(path);
                self.status = self.export.message.clone();
                self.level = Level::Ok;
            }
            Err(error) => {
                self.export.message = format!("{label} failed: {error}");
                self.status = self.export.message.clone();
                self.level = Level::Error;
            }
        }
    }

    fn open_last_export(&mut self) {
        if let Some(path) = &self.export.last {
            crate::cli::plot::open_file(path);
        }
    }

    fn apply_edit(&mut self, edit: Edit) {
        match edit.target {
            EditTarget::Field(kind) => {
                self.model.set_field(kind, &edit.buffer);
                self.validation = self.model.validate();
            }
            EditTarget::Sweep { row, column } => {
                if let Some(range) = self.sweeps.ranges.get_mut(row) {
                    match column {
                        0 => range.name = edit.buffer.trim().to_string(),
                        1 => range.start = edit.buffer.trim().to_string(),
                        2 => range.end = edit.buffer.trim().to_string(),
                        _ => range.step = edit.buffer.trim().to_string(),
                    }
                }
                self.sweeps.message.clear();
            }
        }
        self.status = format!("{} = {}", edit.label, edit.buffer);
        self.level = Level::Info;
    }

    fn move_selection(&mut self, delta: i32) {
        let count = self.model.fields().len();
        if count == 0 {
            return;
        }
        let next = (self.selected as i32 + delta).clamp(0, count as i32 - 1) as usize;
        self.selected = next;
        self.table_state.select(Some(next));
    }

    fn cycle(&mut self, delta: i32) {
        let fields = self.model.fields();
        let Some(field) = fields.get(self.selected) else {
            return;
        };
        self.model.cycle_field(field.kind, delta);
        self.validation = self.model.validate();
    }

    fn activate(&mut self) {
        let fields = self.model.fields();
        let Some(field) = fields.get(self.selected) else {
            return;
        };
        match field.kind {
            FieldKind::Quadrature
            | FieldKind::Scheme
            | FieldKind::Mode
            | FieldKind::ReflexLeft
            | FieldKind::ReflexRight => {
                self.cycle(1);
            }
            FieldKind::NumRegions | FieldKind::NumZones => {
                self.start_edit(field.kind, &field.label, &field.value);
            }
            kind => self.start_edit(kind, &field.label, &field.value),
        }
    }

    fn start_edit(&mut self, kind: FieldKind, label: &str, value: &str) {
        self.editing = Some(Edit {
            target: EditTarget::Field(kind),
            label: label.to_string(),
            buffer: value.to_string(),
        });
    }

    fn start_sweep_edit(&mut self, row: usize, column: usize) {
        let Some(range) = self.sweeps.ranges.get(row) else {
            return;
        };
        let (label, value) = match column {
            0 => ("placeholder name", range.name.clone()),
            1 => ("start", range.start.clone()),
            2 => ("end", range.end.clone()),
            _ => ("step", range.step.clone()),
        };
        self.editing = Some(Edit {
            target: EditTarget::Sweep { row, column },
            label: format!("range {} {label}", row + 1),
            buffer: value,
        });
    }

    /// Draw the whole interface.
    pub fn render(&mut self, frame: &mut Frame) {
        let area = frame.area();
        if area.width < 80 || area.height < 24 {
            let message = Paragraph::new(format!(
                "Terminal too small ({}x{}).\nMinimum: 80x24.",
                area.width, area.height
            ))
            .alignment(ratatui::layout::Alignment::Center)
            .block(Block::bordered().title("nts-rs"));
            frame.render_widget(message, area);
            return;
        }

        let chunks = Layout::vertical([
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Min(0),
            Constraint::Length(2),
        ])
        .split(area);

        self.render_header(frame, chunks[0]);
        self.render_tabs(frame, chunks[1]);
        match self.tab {
            Tab::Config => self.render_config(frame, chunks[2]),
            Tab::Run => self.render_run(frame, chunks[2]),
            Tab::Results => self.render_results(frame, chunks[2]),
            Tab::Placeholders => self.render_sweeps(frame, chunks[2]),
            Tab::Plots => self.render_plots(frame, chunks[2]),
            Tab::Export => self.render_export(frame, chunks[2]),
        }
        self.render_footer(frame, chunks[3]);

        if self.editing.is_some() {
            self.render_editor(frame);
        }
    }

    fn render_header(&self, frame: &mut Frame, area: Rect) {
        let header = Line::from(vec![
            Span::styled(
                " nts-rs ",
                Style::default()
                    .bg(Color::Blue)
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(" 1D S_N neutron transport "),
            Span::styled(
                format!("  {}", paths::work_root().display()),
                Style::default().fg(Color::DarkGray),
            ),
        ]);
        frame.render_widget(Paragraph::new(header), area);
    }

    fn render_tabs(&self, frame: &mut Frame, area: Rect) {
        let tabs = Tabs::new(Tab::ALL.iter().map(|tab| tab.title()).collect::<Vec<_>>())
            .select(self.tab.index())
            .highlight_style(
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            );
        frame.render_widget(tabs, area);
    }

    fn render_config(&mut self, frame: &mut Frame, area: Rect) {
        let columns = Layout::horizontal([Constraint::Percentage(62), Constraint::Percentage(38)])
            .split(area);
        self.render_form(frame, columns[0]);
        self.render_side_panel(frame, columns[1]);
    }

    fn render_form(&mut self, frame: &mut Frame, area: Rect) {
        let fields = self.model.fields();
        let rows = fields
            .iter()
            .enumerate()
            .map(|(index, field)| {
                let style = if index == self.selected {
                    Style::default()
                        .bg(Color::DarkGray)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default()
                };
                Row::new(vec![
                    Cell::from(field.section),
                    Cell::from(field.label.clone()),
                    Cell::from(field.value.clone()),
                ])
                .style(style)
            })
            .collect::<Vec<_>>();

        let table = Table::new(
            rows,
            [
                Constraint::Length(11),
                Constraint::Length(22),
                Constraint::Min(0),
            ],
        )
        .header(
            Row::new(vec!["Section", "Parameter", "Value"]).style(
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ),
        )
        .block(Block::bordered().title("Configuration"));

        frame.render_stateful_widget(table, area, &mut self.table_state);
    }

    fn render_side_panel(&self, frame: &mut Frame, area: Rect) {
        let chunks = Layout::vertical([
            Constraint::Length(7),
            Constraint::Length(6),
            Constraint::Min(0),
        ])
        .split(area);

        let (text, color) = match &self.validation {
            Validation::Valid(config) => (
                format!(
                    "✓ Valid\ncells: {}\nmode: {}\nscheme: {}",
                    config.ntc,
                    config.mode.as_str(),
                    config.method().as_str()
                ),
                Color::Green,
            ),
            Validation::Template(names) => (
                format!("⚠ Template\nplaceholders: {}", names.join(", ")),
                Color::Yellow,
            ),
            Validation::Invalid(error) => (format!("✗ Invalid\n{error}"), Color::Red),
        };
        let status = Paragraph::new(text)
            .style(Style::default().fg(color))
            .wrap(Wrap { trim: true })
            .block(Block::bordered().title("Validation"));
        frame.render_widget(status, chunks[0]);

        let geometry = Paragraph::new(vec![
            Line::from(geometry_bar(&self.model)),
            Line::from(format!("total: {:.3} cm", self.model.total_thickness())),
        ])
        .wrap(Wrap { trim: true })
        .block(Block::bordered().title("Geometry"));
        frame.render_widget(geometry, chunks[1]);

        let help = Paragraph::new(
            "Numeric fields accept placeholders (kk, mm).\n\
             ←/→ cycles enumerations.\n\
             s saves to outputs/inputs/config_1d_###.json.",
        )
        .style(Style::default().fg(Color::DarkGray))
        .wrap(Wrap { trim: true })
        .block(Block::bordered().title("Tips"));
        frame.render_widget(help, chunks[2]);
    }

    fn render_run(&self, frame: &mut Frame, area: Rect) {
        let columns = Layout::horizontal([Constraint::Percentage(36), Constraint::Percentage(64)])
            .split(area);

        let offset = self.run.selected.saturating_sub(8) as u16;
        let lines: Vec<Line> = self
            .run
            .entries
            .iter()
            .enumerate()
            .map(|(index, entry)| {
                let selected = index == self.run.selected;
                let style = if selected {
                    Style::default()
                        .bg(Color::DarkGray)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default()
                };
                Line::from(Span::styled(
                    format!("{} {}", if selected { "▶" } else { " " }, entry.name),
                    style,
                ))
            })
            .collect();
        let list = Paragraph::new(lines)
            .block(Block::bordered().title(format!("Configs ({})", self.run.entries.len())))
            .scroll((offset, 0));
        frame.render_widget(list, columns[0]);

        let chunks =
            Layout::vertical([Constraint::Length(3), Constraint::Min(0)]).split(columns[1]);
        let state = if self.run.running { "running" } else { "idle" };
        let header = Paragraph::new(Line::from(vec![
            Span::styled(
                format!(" {state} "),
                Style::default().fg(if self.run.running {
                    Color::Yellow
                } else {
                    Color::DarkGray
                }),
            ),
            Span::raw(self.run.current.clone().unwrap_or_default()),
            Span::styled(
                self.run
                    .output
                    .as_ref()
                    .map(|path| format!("  → {}", path.display()))
                    .unwrap_or_default(),
                Style::default().fg(Color::DarkGray),
            ),
        ]))
        .block(Block::bordered().title("Run"));
        frame.render_widget(header, chunks[0]);

        let start = self.run.log.len().saturating_sub(30);
        let log_lines: Vec<Line> = self.run.log[start..]
            .iter()
            .map(|line| Line::from(line.clone()))
            .collect();
        let log = Paragraph::new(log_lines)
            .wrap(Wrap { trim: false })
            .block(Block::bordered().title("Log"));
        frame.render_widget(log, chunks[1]);
    }

    fn render_results(&self, frame: &mut Frame, area: Rect) {
        let sections = Layout::vertical([Constraint::Length(6), Constraint::Min(0)]).split(area);

        let offset = self.results.selected.saturating_sub(8) as u16;
        let lines: Vec<Line> = self
            .results
            .entries
            .iter()
            .enumerate()
            .map(|(index, entry)| {
                let selected = index == self.results.selected;
                let style = if selected {
                    Style::default()
                        .bg(Color::DarkGray)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default()
                };
                Line::from(Span::styled(
                    format!(
                        "{} {} ({} run(s))",
                        if selected { "▶" } else { " " },
                        entry.name,
                        entry.total
                    ),
                    style,
                ))
            })
            .collect();
        let list = Paragraph::new(lines)
            .block(Block::bordered().title(format!("Outputs ({})", self.results.entries.len())))
            .scroll((offset, 0));
        frame.render_widget(list, sections[0]);

        match &self.results.payload {
            Some(payload) => {
                let rows = payload
                    .get("runs")
                    .and_then(Value::as_array)
                    .map(|runs| {
                        runs.iter()
                            .map(|run| {
                                Row::new(vec![
                                    Cell::from(
                                        run.get("config_name")
                                            .and_then(Value::as_str)
                                            .unwrap_or("?"),
                                    ),
                                    Cell::from(
                                        run.get("mode").and_then(Value::as_str).unwrap_or("-"),
                                    ),
                                    Cell::from(
                                        run.get("spatial_scheme")
                                            .and_then(Value::as_str)
                                            .unwrap_or("-"),
                                    ),
                                    Cell::from(format!(
                                        "{}",
                                        run.get("converged")
                                            .and_then(Value::as_bool)
                                            .unwrap_or(false)
                                    )),
                                    Cell::from(
                                        run.get("iteration")
                                            .and_then(Value::as_u64)
                                            .map(|value| value.to_string())
                                            .unwrap_or_else(|| "-".into()),
                                    ),
                                    Cell::from(format_number(run.get("k_eff"))),
                                    Cell::from(format_number(run.get("k_balance"))),
                                ])
                            })
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_default();
                let table = Table::new(
                    rows,
                    [
                        Constraint::Length(22),
                        Constraint::Length(14),
                        Constraint::Length(8),
                        Constraint::Length(9),
                        Constraint::Length(7),
                        Constraint::Length(12),
                        Constraint::Length(12),
                    ],
                )
                .header(Row::new(vec![
                    "Config",
                    "Mode",
                    "Scheme",
                    "Conv",
                    "Iter",
                    "k_eff",
                    "k_balance",
                ]))
                .block(Block::bordered().title("Runs"));
                frame.render_widget(table, sections[1]);
            }
            None => {
                let message =
                    Paragraph::new("No output selected.").block(Block::bordered().title("Runs"));
                frame.render_widget(message, sections[1]);
            }
        }
    }

    fn render_sweeps(&self, frame: &mut Frame, area: Rect) {
        let sections = Layout::vertical([Constraint::Min(0), Constraint::Length(3)]).split(area);
        let rows = self
            .sweeps
            .ranges
            .iter()
            .enumerate()
            .map(|(index, range)| {
                let selected = index == self.sweeps.selected;
                let style = if selected {
                    Style::default()
                        .bg(Color::DarkGray)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default()
                };
                let cell = |column: usize, text: &str| {
                    if selected && column == self.sweeps.column {
                        Cell::from(Span::styled(
                            text.to_string(),
                            Style::default().fg(Color::Yellow),
                        ))
                    } else {
                        Cell::from(text.to_string())
                    }
                };
                Row::new(vec![
                    cell(0, &range.name),
                    cell(1, &range.start),
                    cell(2, &range.end),
                    cell(3, &range.step),
                ])
                .style(style)
            })
            .collect::<Vec<_>>();

        let table = Table::new(
            rows,
            [
                Constraint::Length(18),
                Constraint::Length(12),
                Constraint::Length(12),
                Constraint::Length(12),
            ],
        )
        .header(Row::new(vec!["Name", "Start", "End", "Step"]))
        .block(Block::bordered().title("Placeholders"));
        frame.render_widget(table, sections[0]);

        let info = Paragraph::new(vec![
            Line::from(self.sweeps.message.clone()),
            Line::from(Span::styled(
                format!(
                    "base config: {}",
                    self.run
                        .entries
                        .get(self.run.selected)
                        .map_or("<none>", |entry| entry.name.as_str())
                ),
                Style::default().fg(Color::DarkGray),
            )),
        ])
        .wrap(Wrap { trim: true })
        .block(Block::bordered().title("Expansion"));
        frame.render_widget(info, sections[1]);
    }

    fn render_plots(&mut self, frame: &mut Frame, area: Rect) {
        let columns = Layout::horizontal([Constraint::Percentage(42), Constraint::Percentage(58)])
            .split(area);
        let info = Paragraph::new(vec![
            Line::from(format!("kind: {}", PLOT_KINDS[self.plots.kind])),
            Line::from(format!(
                "output: {}",
                self.results
                    .entries
                    .get(self.results.selected)
                    .map_or("<none>", |entry| entry.name.as_str())
            )),
            Line::from(""),
            Line::from(self.plots.message.clone()),
            Line::from(Span::styled(
                self.plots
                    .last
                    .as_ref()
                    .map(|path| path.display().to_string())
                    .unwrap_or_default(),
                Style::default().fg(Color::DarkGray),
            )),
        ])
        .wrap(Wrap { trim: true })
        .block(Block::bordered().title("Plot"));
        frame.render_widget(info, columns[0]);

        #[cfg(feature = "images")]
        {
            match self.plots.preview.as_mut() {
                Some(protocol) => {
                    frame.render_stateful_widget(
                        ratatui_image::StatefulImage::default(),
                        columns[1],
                        protocol,
                    );
                }
                None => {
                    let message =
                        Paragraph::new("Generate a plot to preview it (feature `images`).")
                            .block(Block::bordered().title("Preview"));
                    frame.render_widget(message, columns[1]);
                }
            }
        }
        #[cfg(not(feature = "images"))]
        {
            let message = Paragraph::new(
                "Preview disabled. Build with `--features images` for an inline preview.",
            )
            .block(Block::bordered().title("Preview"));
            frame.render_widget(message, columns[1]);
        }
    }

    fn render_export(&self, frame: &mut Frame, area: Rect) {
        let info = Paragraph::new(vec![
            Line::from("d  datos.json (positions 0 and last)"),
            Line::from("x  Excel workbook next to the selected output"),
            Line::from("s  SCS plot in outputs/results/plots"),
            Line::from(""),
            Line::from(format!(
                "selected output: {}",
                self.results
                    .entries
                    .get(self.results.selected)
                    .map_or("<none>", |entry| entry.name.as_str())
            )),
            Line::from(""),
            Line::from(self.export.message.clone()),
        ])
        .wrap(Wrap { trim: true })
        .block(Block::bordered().title("Export"));
        frame.render_widget(info, area);
    }

    fn render_footer(&self, frame: &mut Frame, area: Rect) {
        let help = match self.tab {
            Tab::Config => {
                "↑/↓ move   ←/→ cycle   Enter edit/toggle   s save   v validate   Tab switch   q quit"
            }
            Tab::Run => "↑/↓ select   r run   c cancel   Tab switch   q quit",
            Tab::Results => "↑/↓ select output   Tab switch   q quit",
            Tab::Placeholders => "a add   d delete   ←/→ column   Enter edit   x expand   q quit",
            Tab::Plots => "←/→/k kind   g generate   o open   Tab switch   q quit",
            Tab::Export => "d data   x excel   s scs plot   o open   Tab switch   q quit",
        };
        let level_style = match self.level {
            Level::Info => Style::default().fg(Color::Gray),
            Level::Ok => Style::default().fg(Color::Green),
            Level::Error => Style::default().fg(Color::Red),
        };
        let lines = vec![
            Line::from(Span::styled(help, Style::default().fg(Color::DarkGray))),
            Line::from(Span::styled(self.status.clone(), level_style)),
        ];
        frame.render_widget(Paragraph::new(lines), area);
    }

    fn render_editor(&self, frame: &mut Frame) {
        let Some(edit) = &self.editing else {
            return;
        };
        let area = centered_rect(frame.area(), 60, 3);
        frame.render_widget(Clear, area);
        let block = Block::bordered().title(format!("Edit {}", edit.label));
        let paragraph = Paragraph::new(format!("{}▏", edit.buffer)).block(block);
        frame.render_widget(paragraph, area);
    }
}

fn centered_rect(area: Rect, percent_x: u16, height: u16) -> Rect {
    let horizontal = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(area);
    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Fill(1),
            Constraint::Length(height),
            Constraint::Fill(1),
        ])
        .split(horizontal[1]);
    vertical[1]
}

#[cfg(feature = "images")]
fn load_preview(path: &Path) -> Option<ratatui_image::protocol::StatefulProtocol> {
    use std::io::IsTerminal;

    if !std::io::stdout().is_terminal() {
        return None;
    }
    let picker = ratatui_image::picker::Picker::from_query_stdio().ok()?;
    let image = image::ImageReader::open(path).ok()?.decode().ok()?;
    Some(picker.new_resize_protocol(image))
}

fn format_number(value: Option<&Value>) -> String {
    value
        .and_then(Value::as_f64)
        .map_or_else(|| "-".to_string(), |number| format!("{number:.6}"))
}

fn geometry_bar(model: &ConfigModel) -> String {
    let widths: Vec<f64> = model
        .hr
        .iter()
        .filter_map(|value| value.trim().parse::<f64>().ok())
        .filter(|value| *value > 0.0)
        .collect();
    if widths.is_empty() {
        return "[no geometry]".to_string();
    }
    let total: f64 = widths.iter().sum();
    let slots = 34usize;
    let mut bar = String::from("[");
    for (index, width) in widths.iter().enumerate() {
        let cells = ((width / total) * slots as f64).round().max(1.0) as usize;
        if index > 0 {
            bar.push('│');
        }
        bar.push_str(&"█".repeat(cells));
    }
    bar.push(']');
    bar.push_str(&format!(" {total:.2} cm"));
    bar
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tabs_cycle() {
        assert_eq!(Tab::Config.next(), Tab::Run);
        assert_eq!(Tab::Config.previous(), Tab::Export);
    }

    #[test]
    fn geometry_bar_has_regions() {
        let model = ConfigModel::default();
        let bar = geometry_bar(&model);
        assert!(bar.starts_with('['));
        assert!(bar.contains('█'));
        assert!(bar.contains("20.00 cm"));
    }
}
