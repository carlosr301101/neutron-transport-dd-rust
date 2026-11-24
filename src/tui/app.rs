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

/// In-progress cell edit.
#[derive(Debug, Clone)]
pub struct Edit {
    kind: FieldKind,
    label: String,
    buffer: String,
}

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
            _ => {}
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

    fn apply_edit(&mut self, edit: Edit) {
        self.model.set_field(edit.kind, &edit.buffer);
        self.validation = self.model.validate();
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
            kind,
            label: label.to_string(),
            buffer: value.to_string(),
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
            tab => render_placeholder_tab(frame, chunks[2], tab),
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

    fn render_footer(&self, frame: &mut Frame, area: Rect) {
        let help = match self.tab {
            Tab::Config => {
                "↑/↓ move   ←/→ cycle   Enter edit/toggle   s save   v validate   Tab switch   q quit"
            }
            Tab::Run => "↑/↓ select   r run   c cancel   Tab switch   q quit",
            Tab::Results => "↑/↓ select output   Tab switch   q quit",
            _ => "Tab switch   q quit",
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

fn render_placeholder_tab(frame: &mut Frame, area: Rect, tab: Tab) {
    let message = Paragraph::new(format!(
        "{} screen\n\nThis screen is implemented in the next phase.",
        tab.title()
    ))
    .alignment(ratatui::layout::Alignment::Center)
    .block(Block::bordered().title(tab.title()));
    let rows = Layout::vertical([
        Constraint::Fill(1),
        Constraint::Length(5),
        Constraint::Fill(1),
    ])
    .split(area);
    frame.render_widget(message, rows[1]);
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
