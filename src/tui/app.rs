//! TUI application state, input handling and rendering.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;
use std::time::Duration;

use ratatui::Frame;
use ratatui::crossterm::event::{
    self, Event, KeyCode, KeyEvent, KeyEventKind, MouseButton, MouseEvent, MouseEventKind,
};
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{
    Block, Cell, Clear, List, ListItem, Paragraph, Row, Table, TableState, Wrap,
};
use serde_json::Value;

use crate::config::{Config, RunOverrides};
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

/// Collapsible section of the configuration form.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum SectionId {
    /// Regions and cell counts.
    Geometry,
    /// Per-zone materials.
    Materials,
    /// Per-region external source.
    Source,
    /// Quadrature order and scheme.
    Quadrature,
    /// Boundary conditions.
    Boundaries,
    /// Physics mode and outer iteration.
    Physics,
    /// Power normalization.
    Power,
    /// Convergence tolerances.
    Convergence,
    /// Presentation metadata.
    Metadata,
}

impl SectionId {
    /// Sections in display order.
    pub const ALL: [SectionId; 9] = [
        SectionId::Geometry,
        SectionId::Materials,
        SectionId::Source,
        SectionId::Quadrature,
        SectionId::Boundaries,
        SectionId::Physics,
        SectionId::Power,
        SectionId::Convergence,
        SectionId::Metadata,
    ];

    /// Section title.
    pub const fn title(self) -> &'static str {
        match self {
            SectionId::Geometry => "Geometry",
            SectionId::Materials => "Materials",
            SectionId::Source => "Source",
            SectionId::Quadrature => "Quadrature",
            SectionId::Boundaries => "Boundaries",
            SectionId::Physics => "Physics",
            SectionId::Power => "Power",
            SectionId::Convergence => "Convergence",
            SectionId::Metadata => "Metadata",
        }
    }

    fn from_label(label: &str) -> SectionId {
        match label {
            "Geometry" => SectionId::Geometry,
            "Materials" => SectionId::Materials,
            "Source" => SectionId::Source,
            "Quadrature" => SectionId::Quadrature,
            "Boundaries" => SectionId::Boundaries,
            "Physics" => SectionId::Physics,
            "Power" => SectionId::Power,
            "Convergence" => SectionId::Convergence,
            _ => SectionId::Metadata,
        }
    }
}

/// Selected row of the configuration form.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Selection {
    /// A collapsible section header.
    Section(SectionId),
    /// A collapsible zone header.
    Zone(usize),
    /// A field row.
    Field(FieldKind),
}

/// Focus of the configuration screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfigFocus {
    /// Editing the form.
    Form,
    /// Browsing saved configurations.
    Saved,
}

/// What the path dialog is used for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DialogPurpose {
    SaveConfig,
    ExpansionOutput,
}

#[derive(Debug, Clone)]
struct PathDialog {
    purpose: DialogPurpose,
    input: TextInput,
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

/// Single-line text buffer with a character cursor.
#[derive(Debug, Clone)]
struct TextInput {
    buffer: String,
    cursor: usize,
}

impl TextInput {
    fn new(text: &str) -> Self {
        TextInput {
            buffer: text.to_string(),
            cursor: text.chars().count(),
        }
    }

    fn text(&self) -> &str {
        &self.buffer
    }

    fn len(&self) -> usize {
        self.buffer.chars().count()
    }

    fn move_left(&mut self) {
        self.cursor = self.cursor.saturating_sub(1);
    }

    fn move_right(&mut self) {
        if self.cursor < self.len() {
            self.cursor += 1;
        }
    }

    fn home(&mut self) {
        self.cursor = 0;
    }

    fn end(&mut self) {
        self.cursor = self.len();
    }

    fn insert(&mut self, character: char) {
        let index = byte_index(&self.buffer, self.cursor);
        self.buffer.insert(index, character);
        self.cursor += 1;
    }

    fn backspace(&mut self) {
        if self.cursor == 0 {
            return;
        }
        let start = byte_index(&self.buffer, self.cursor - 1);
        let end = byte_index(&self.buffer, self.cursor);
        self.buffer.replace_range(start..end, "");
        self.cursor -= 1;
    }

    fn delete(&mut self) {
        if self.cursor >= self.len() {
            return;
        }
        let start = byte_index(&self.buffer, self.cursor);
        let end = byte_index(&self.buffer, self.cursor + 1);
        self.buffer.replace_range(start..end, "");
    }
}

fn byte_index(text: &str, character: usize) -> usize {
    text.char_indices()
        .nth(character)
        .map_or(text.len(), |(index, _)| index)
}

fn text_input_key(input: &mut TextInput, code: KeyCode) -> bool {
    match code {
        KeyCode::Left => input.move_left(),
        KeyCode::Right => input.move_right(),
        KeyCode::Home => input.home(),
        KeyCode::End => input.end(),
        KeyCode::Backspace => input.backspace(),
        KeyCode::Delete => input.delete(),
        KeyCode::Char(character) => input.insert(character),
        _ => return false,
    }
    true
}

/// In-progress cell edit.
#[derive(Debug, Clone)]
pub struct Edit {
    target: EditTarget,
    label: String,
    input: TextInput,
}

/// Dropdown of an enumerated field.
#[derive(Debug, Clone)]
struct SelectPopup {
    field: FieldKind,
    title: String,
    options: Vec<String>,
    selected: usize,
}

/// One visible row of the form.
#[derive(Debug, Clone)]
struct FormRow {
    selection: Selection,
    label: String,
    value: String,
    header: bool,
    depth: usize,
}

/// Action attached to a clickable rectangle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    /// Switch to a tab.
    Tab(Tab),
    /// Select (and toggle/activate) a form row.
    FormRow(usize),
    /// Select a configuration in the run list.
    RunList(usize),
    /// Select an output in the results list.
    ResultList(usize),
    /// Select a range in the sweep table.
    SweepRow(usize),
    /// Start the selected run.
    Run,
    /// Cancel the active run.
    Cancel,
    /// Generate the selected plot.
    GeneratePlot,
    /// Open the last plot.
    OpenPlot,
    /// Cycle the plot kind.
    NextKind,
    /// Export `datos.json`.
    ExportData,
    /// Export Excel.
    ExportExcel,
    /// Export the SCS plot.
    ExportScs,
    /// Open the last export.
    OpenExport,
    /// Pick an option from the dropdown.
    SelectOption(usize),
    /// Load a saved configuration.
    LoadSaved(usize),
    /// Select one configuration from the placeholder list.
    SelectPlaceholder(usize),
    /// Save the expanded configuration list.
    SaveExpansion,
    /// Run all expanded configurations.
    RunBatch,
    /// Edit the expansion output path.
    EditOutput,
}

/// Panel under the mouse (used by the wheel).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Panel {
    Form,
    RunList,
    RunLog,
    ResultsList,
    ResultsTable,
    SweepTable,
    PlaceholderConfigs,
    PlotInfo,
    ExportInfo,
    Saved,
}

/// Clickable regions of the last frame.
#[derive(Debug, Default)]
struct HitMap {
    actions: Vec<(Rect, Action)>,
    panels: Vec<(Rect, Panel)>,
}

impl HitMap {
    fn action_at(&self, column: u16, row: u16) -> Option<Action> {
        self.actions
            .iter()
            .rev()
            .find(|(rect, _)| contains(*rect, column, row))
            .map(|(_, action)| *action)
    }

    fn panel_at(&self, column: u16, row: u16) -> Option<Panel> {
        self.panels
            .iter()
            .rev()
            .find(|(rect, _)| contains(*rect, column, row))
            .map(|(_, panel)| *panel)
    }
}

fn contains(rect: Rect, column: u16, row: u16) -> bool {
    column >= rect.x && column < rect.x + rect.width && row >= rect.y && row < rect.y + rect.height
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
    /// Scroll offset of the log (lines from the bottom).
    pub log_offset: usize,
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
    /// Scroll offset of the runs table.
    pub table_offset: usize,
}

/// Placeholder ranges being edited.
pub struct SweepState {
    /// Detected ranges.
    pub ranges: Vec<SweepRange>,
    /// Selected range.
    pub selected: usize,
    /// Selected column (0 start, 1 end, 2 step).
    pub column: usize,
    /// Last expansion message.
    pub message: String,
    /// Scroll offset of the range table.
    pub scroll: usize,
    /// Saved configuration used as the source.
    pub config_selected: usize,
    /// Expansion output path.
    pub output: PathBuf,
    /// Indices into `App::saved` that contain placeholders.
    pub placeholder_entries: Vec<usize>,
    /// Detected placeholder names per filtered entry.
    pub placeholder_names: Vec<Vec<String>>,
}

impl Default for SweepState {
    fn default() -> Self {
        SweepState {
            ranges: Vec::new(),
            selected: 0,
            column: 0,
            message: String::new(),
            scroll: 0,
            config_selected: 0,
            output: paths::outputs_dir().join("inputs").join("inputs_1d.json"),
            placeholder_entries: Vec::new(),
            placeholder_names: Vec::new(),
        }
    }
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

/// Application state.
pub struct App {
    /// Active tab.
    pub tab: Tab,
    /// Configuration being edited.
    pub model: ConfigModel,
    /// Selected form row.
    pub selection: Selection,
    /// Persistent table state (keeps the scroll offset).
    pub table_state: TableState,
    /// Active edit, if any.
    pub editing: Option<Edit>,
    /// Active dropdown, if any.
    select: Option<SelectPopup>,
    /// Active path dialog (save-as or expansion output).
    save_dialog: Option<PathDialog>,
    /// Saved configurations (list items expanded).
    pub saved: Vec<config_store::ConfigEntry>,
    /// Selected saved configuration.
    pub saved_selected: usize,
    /// Focus of the configuration screen.
    pub config_focus: ConfigFocus,
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
    /// Collapsed sections.
    pub collapsed_sections: BTreeSet<SectionId>,
    /// Collapsed zone groups.
    pub collapsed_zones: BTreeSet<usize>,
    /// Mouse capture requested by the user.
    pub mouse_captured: bool,
    mouse_applied: bool,
    hitmap: HitMap,
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
        let mut collapsed_zones = BTreeSet::new();
        if model.num_zones > 4 {
            collapsed_zones.extend(1..model.num_zones);
        }
        App {
            tab: Tab::Config,
            model,
            selection: Selection::Field(FieldKind::NumRegions),
            table_state: TableState::default().with_selected(Some(0)),
            editing: None,
            select: None,
            save_dialog: None,
            saved: config_store::list(),
            saved_selected: 0,
            config_focus: ConfigFocus::Form,
            validation,
            status: "Ready. Press ? or see the footer for keys.".into(),
            level: Level::Info,
            quit: false,
            run,
            results,
            sweeps: SweepState::default(),
            plots: PlotState::default(),
            export: ExportState::default(),
            collapsed_sections: BTreeSet::new(),
            collapsed_zones,
            mouse_captured: true,
            mouse_applied: false,
            hitmap: HitMap::default(),
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
        self.sync_mouse()?;
        let outcome = (|| -> std::io::Result<()> {
            loop {
                terminal.draw(|frame| self.render(frame))?;
                if event::poll(Duration::from_millis(200))? {
                    match event::read()? {
                        Event::Key(key) if key.kind == KeyEventKind::Press => {
                            self.handle_key(key);
                            self.sync_mouse()?;
                        }
                        Event::Mouse(mouse) => self.handle_mouse(mouse),
                        _ => {}
                    }
                }
                self.poll_runner();
                if self.quit {
                    break;
                }
            }
            Ok(())
        })();
        if self.mouse_applied {
            let _ = ratatui::crossterm::execute!(
                std::io::stdout(),
                ratatui::crossterm::event::DisableMouseCapture
            );
            self.mouse_applied = false;
        }
        outcome
    }

    fn sync_mouse(&mut self) -> std::io::Result<()> {
        if self.mouse_captured == self.mouse_applied {
            return Ok(());
        }
        let mut out = std::io::stdout();
        if self.mouse_captured {
            ratatui::crossterm::execute!(out, ratatui::crossterm::event::EnableMouseCapture)?;
        } else {
            ratatui::crossterm::execute!(out, ratatui::crossterm::event::DisableMouseCapture)?;
        }
        self.mouse_applied = self.mouse_captured;
        Ok(())
    }

    /// Handle one key press.
    pub fn handle_key(&mut self, key: KeyEvent) {
        if key.kind != KeyEventKind::Press {
            return;
        }

        if self.select.is_some() {
            self.handle_popup_key(key);
            return;
        }

        if self.save_dialog.is_some() {
            self.handle_save_dialog_key(key);
            return;
        }

        if let Some(edit) = self.editing.as_mut() {
            match key.code {
                KeyCode::Esc => self.editing = None,
                KeyCode::Enter => {
                    let edit = self.editing.take().expect("edit");
                    self.apply_edit(edit);
                }
                code => {
                    text_input_key(&mut edit.input, code);
                }
            }
            return;
        }

        match key.code {
            KeyCode::Char('q') => {
                self.quit = true;
                return;
            }
            KeyCode::Char('m') => {
                self.mouse_captured = !self.mouse_captured;
                self.status = format!(
                    "Mouse capture {}.",
                    if self.mouse_captured {
                        "enabled"
                    } else {
                        "disabled"
                    }
                );
                self.level = Level::Info;
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
            Tab::Config => self.handle_config_key(key.code),
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
                KeyCode::Right => self.sweeps.column = (self.sweeps.column + 1).min(2),
                KeyCode::Char('[') => self.select_placeholder_config(-1),
                KeyCode::Char(']') => self.select_placeholder_config(1),
                KeyCode::Char('r') => self.scan_placeholders(),
                KeyCode::Char('x') | KeyCode::Char('p') => self.open_output_dialog(),
                KeyCode::Char('R') => self.run_batch(),
                KeyCode::Enter => self.edit_sweep_cell(),
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

    fn handle_config_key(&mut self, code: KeyCode) {
        if self.config_focus == ConfigFocus::Saved {
            match code {
                KeyCode::Up => self.move_saved(-1),
                KeyCode::Down => self.move_saved(1),
                KeyCode::Enter => self.load_saved(self.saved_selected),
                KeyCode::Esc | KeyCode::Char('L') => self.config_focus = ConfigFocus::Form,
                _ => {}
            }
            return;
        }

        match code {
            KeyCode::Up => self.move_selection(-1),
            KeyCode::Down => self.move_selection(1),
            KeyCode::Left => self.left_right(-1),
            KeyCode::Right => self.left_right(1),
            KeyCode::Enter | KeyCode::Char(' ') => self.activate(),
            KeyCode::Char('e') => self.edit_selected_field(),
            KeyCode::Char('s') => self.save(),
            KeyCode::Char('v') => self.revalidate(),
            KeyCode::Char('z') => self.toggle_all_zones(),
            KeyCode::Char('Z') => self.toggle_all_sections(),
            KeyCode::Char('L') if !self.saved.is_empty() => {
                self.refresh_saved();
                self.config_focus = ConfigFocus::Saved;
                self.status = "Select a saved configuration (Enter loads it).".into();
                self.level = Level::Info;
            }
            _ => {}
        }
    }

    fn move_saved(&mut self, delta: i32) {
        if self.saved.is_empty() {
            return;
        }
        let count = self.saved.len() as i32;
        self.saved_selected = (self.saved_selected as i32 + delta).clamp(0, count - 1) as usize;
    }

    /// Handle a mouse event (click or wheel).
    pub fn handle_mouse(&mut self, mouse: MouseEvent) {
        if self.save_dialog.is_some() {
            return;
        }

        if self.select.is_some() {
            match mouse.kind {
                MouseEventKind::Down(MouseButton::Left) => {
                    match self.hitmap.action_at(mouse.column, mouse.row) {
                        Some(Action::SelectOption(index)) => {
                            if let Some(popup) = self.select.as_mut() {
                                popup.selected = index;
                            }
                            self.apply_select();
                        }
                        _ => self.select = None,
                    }
                }
                MouseEventKind::ScrollDown => {
                    if let Some(popup) = self.select.as_mut() {
                        popup.selected = (popup.selected + 1) % popup.options.len().max(1);
                    }
                }
                MouseEventKind::ScrollUp => {
                    if let Some(popup) = self.select.as_mut() {
                        let len = popup.options.len().max(1);
                        popup.selected = (popup.selected + len - 1) % len;
                    }
                }
                _ => {}
            }
            return;
        }

        if self.editing.is_some() && matches!(mouse.kind, MouseEventKind::Down(MouseButton::Left)) {
            let edit = self.editing.take().expect("edit");
            self.apply_edit(edit);
        }

        match mouse.kind {
            MouseEventKind::Down(MouseButton::Left) => {
                if let Some(action) = self.hitmap.action_at(mouse.column, mouse.row) {
                    self.perform(action);
                }
            }
            MouseEventKind::ScrollDown => self.scroll_at(mouse.column, mouse.row, 1),
            MouseEventKind::ScrollUp => self.scroll_at(mouse.column, mouse.row, -1),
            _ => {}
        }
    }

    fn perform(&mut self, action: Action) {
        match action {
            Action::Tab(tab) => self.switch_tab(tab),
            Action::FormRow(index) => {
                let rows = self.rows();
                let Some(row) = rows.get(index) else {
                    return;
                };
                let selection = row.selection;
                let header = row.header;
                if selection == self.selection && !header {
                    self.activate();
                } else {
                    self.selection = selection;
                    if header {
                        self.toggle_selection();
                    }
                }
            }
            Action::RunList(index) => {
                self.run.selected = index;
            }
            Action::ResultList(index) => {
                self.results.selected = index;
                self.results.table_offset = 0;
                self.load_selected_result();
            }
            Action::SweepRow(index) => self.sweeps.selected = index,
            Action::Run => self.start_run(),
            Action::Cancel => self.cancel_run(),
            Action::GeneratePlot => self.generate_plot(),
            Action::OpenPlot => self.open_last_plot(),
            Action::NextKind => self.plots.kind = (self.plots.kind + 1) % PLOT_KINDS.len(),
            Action::ExportData => self.export_data(),
            Action::ExportExcel => self.export_excel(),
            Action::ExportScs => self.export_scs(),
            Action::OpenExport => self.open_last_export(),
            Action::SelectOption(index) => {
                if let Some(popup) = self.select.as_mut() {
                    popup.selected = index;
                }
                self.apply_select();
            }
            Action::LoadSaved(index) => {
                self.config_focus = ConfigFocus::Saved;
                self.saved_selected = index;
                self.load_saved(index);
            }
            Action::SelectPlaceholder(index) => {
                self.sweeps.config_selected = index;
                self.load_placeholder_selection();
            }
            Action::SaveExpansion | Action::EditOutput => self.open_output_dialog(),
            Action::RunBatch => self.run_batch(),
        }
    }

    fn scroll_at(&mut self, column: u16, row: u16, direction: i32) {
        let Some(panel) = self.hitmap.panel_at(column, row) else {
            return;
        };
        match panel {
            Panel::Form => self.move_selection(direction * 3),
            Panel::RunList => self.move_run_selection(direction),
            Panel::RunLog => {
                let max = self.run.log.len().saturating_sub(1);
                self.run.log_offset = if direction < 0 {
                    (self.run.log_offset + 3).min(max)
                } else {
                    self.run.log_offset.saturating_sub(3)
                };
            }
            Panel::ResultsList => self.move_results_selection(direction),
            Panel::ResultsTable => {
                self.results.table_offset = if direction < 0 {
                    self.results.table_offset + 1
                } else {
                    self.results.table_offset.saturating_sub(1)
                };
            }
            Panel::PlaceholderConfigs => {
                self.select_placeholder_config(direction);
            }
            Panel::SweepTable => {
                let max = self.sweeps.ranges.len().saturating_sub(1);
                self.sweeps.scroll = if direction < 0 {
                    (self.sweeps.scroll + 1).min(max)
                } else {
                    self.sweeps.scroll.saturating_sub(1)
                };
            }
            Panel::Saved => {
                if !self.saved.is_empty() {
                    let count = self.saved.len() as i32;
                    self.saved_selected =
                        (self.saved_selected as i32 + direction).clamp(0, count - 1) as usize;
                }
            }
            Panel::PlotInfo | Panel::ExportInfo => {}
        }
    }

    fn on_tab_enter(&mut self) {
        match self.tab {
            Tab::Config => self.refresh_saved(),
            Tab::Run => self.refresh_run_entries(),
            Tab::Results => self.refresh_results(),
            Tab::Placeholders => {
                self.refresh_saved();
                self.scan_placeholders();
            }
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
        self.results.table_offset = 0;
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

    /// Whether a dropdown is currently open.
    pub fn popup_open(&self) -> bool {
        self.select.is_some()
    }

    /// Clickable regions of the last rendered frame (for tests automation).
    pub fn hotspots(&self) -> Vec<(Rect, Action)> {
        self.hitmap.actions.clone()
    }

    /// Switch to a tab and refresh its data.
    pub fn switch_tab(&mut self, tab: Tab) {
        self.tab = tab;
        self.on_tab_enter();
    }

    /// Select one field of the form (used by tests and shortcuts).
    pub fn select_field(&mut self, kind: FieldKind) {
        self.selection = Selection::Field(kind);
    }

    fn move_selection(&mut self, delta: i32) {
        let rows = self.rows();
        if rows.is_empty() {
            return;
        }
        let position = rows
            .iter()
            .position(|row| row.selection == self.selection)
            .unwrap_or(0);
        let next = (position as i32 + delta).clamp(0, rows.len() as i32 - 1) as usize;
        self.selection = rows[next].selection;
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
        self.results.table_offset = 0;
        self.load_selected_result();
    }

    fn move_sweep_selection(&mut self, delta: i32) {
        let count = self.sweeps.ranges.len();
        if count == 0 {
            return;
        }
        self.sweeps.selected =
            (self.sweeps.selected as i32 + delta).clamp(0, count as i32 - 1) as usize;
    }

    fn left_right(&mut self, delta: i32) {
        match self.selection {
            Selection::Section(section) => {
                if delta < 0 {
                    self.collapsed_sections.insert(section);
                } else {
                    self.collapsed_sections.remove(&section);
                }
            }
            Selection::Zone(zone) => {
                if delta < 0 {
                    self.collapsed_zones.insert(zone);
                } else {
                    self.collapsed_zones.remove(&zone);
                }
            }
            Selection::Field(kind) => {
                if matches!(
                    kind,
                    FieldKind::Quadrature | FieldKind::Scheme | FieldKind::Mode
                ) {
                    self.model.cycle_field(kind, delta);
                    self.validation = self.model.validate();
                }
            }
        }
    }

    fn toggle_selection(&mut self) {
        match self.selection {
            Selection::Section(section) => self.toggle_section(section),
            Selection::Zone(zone) => self.toggle_zone(zone),
            Selection::Field(kind) => {
                let _ = kind;
            }
        }
    }

    fn toggle_section(&mut self, section: SectionId) {
        if !self.collapsed_sections.remove(&section) {
            self.collapsed_sections.insert(section);
        }
        self.selection = Selection::Section(section);
    }

    fn toggle_zone(&mut self, zone: usize) {
        if !self.collapsed_zones.remove(&zone) {
            self.collapsed_zones.insert(zone);
        }
        self.selection = Selection::Zone(zone);
    }

    fn toggle_all_zones(&mut self) {
        if self.collapsed_zones.len() >= self.model.num_zones {
            self.collapsed_zones.clear();
        } else {
            self.collapsed_zones = (0..self.model.num_zones).collect();
        }
    }

    fn toggle_all_sections(&mut self) {
        if self.collapsed_sections.len() == SectionId::ALL.len() {
            self.collapsed_sections.clear();
        } else {
            self.collapsed_sections = SectionId::ALL.into_iter().collect();
        }
    }

    fn activate(&mut self) {
        match self.selection {
            Selection::Section(_) | Selection::Zone(_) => self.toggle_selection(),
            Selection::Field(kind) => match kind {
                FieldKind::Quadrature
                | FieldKind::Scheme
                | FieldKind::Mode
                | FieldKind::RegionZone(_) => self.open_select(kind),
                FieldKind::ReflexLeft | FieldKind::ReflexRight => {
                    self.model.cycle_field(kind, 1);
                    self.validation = self.model.validate();
                }
                _ => self.edit_selected_field(),
            },
        }
    }

    fn edit_selected_field(&mut self) {
        let Selection::Field(kind) = self.selection else {
            return;
        };
        let rows = self.rows();
        if let Some(row) = rows.iter().find(|row| row.selection == self.selection) {
            self.editing = Some(Edit {
                target: EditTarget::Field(kind),
                label: row.label.clone(),
                input: TextInput::new(&row.value),
            });
        }
    }

    fn open_select(&mut self, kind: FieldKind) {
        let (title, options, current) = match kind {
            FieldKind::RegionZone(region) => {
                let options: Vec<String> = (0..self.model.num_zones)
                    .map(|zone| {
                        let name = self
                            .model
                            .zone_names
                            .get(zone)
                            .filter(|name| !name.is_empty())
                            .cloned()
                            .unwrap_or_else(|| format!("zone {}", zone + 1));
                        format!("{}: {}", zone + 1, name)
                    })
                    .collect();
                let current = self
                    .model
                    .izl
                    .get(region)
                    .and_then(|value| value.trim().parse::<usize>().ok())
                    .unwrap_or(1)
                    .saturating_sub(1)
                    .min(options.len().saturating_sub(1));
                ("Material zone".to_string(), options, current)
            }
            FieldKind::Quadrature => {
                let options: Vec<String> = crate::tui::model::QUADRATURE
                    .iter()
                    .map(|value| value.to_string())
                    .collect();
                let current = crate::tui::model::QUADRATURE
                    .iter()
                    .position(|value| *value == self.model.n)
                    .unwrap_or(0);
                ("Quadrature order".to_string(), options, current)
            }
            FieldKind::Scheme => {
                let options: Vec<String> = crate::tui::model::SCHEMES
                    .iter()
                    .map(|value| (*value).to_string())
                    .collect();
                ("Spatial scheme".to_string(), options, self.model.scheme)
            }
            FieldKind::Mode => {
                let options: Vec<String> = crate::tui::model::MODES
                    .iter()
                    .map(|value| (*value).to_string())
                    .collect();
                ("Physics mode".to_string(), options, self.model.mode)
            }
            _ => return,
        };
        self.select = Some(SelectPopup {
            field: kind,
            title,
            options,
            selected: current,
        });
    }

    fn handle_popup_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc => self.select = None,
            KeyCode::Enter => self.apply_select(),
            KeyCode::Up | KeyCode::BackTab => {
                if let Some(popup) = self.select.as_mut() {
                    popup.selected = popup
                        .selected
                        .checked_sub(1)
                        .unwrap_or(popup.options.len().saturating_sub(1));
                }
            }
            KeyCode::Down | KeyCode::Tab => {
                if let Some(popup) = self.select.as_mut() {
                    popup.selected = (popup.selected + 1) % popup.options.len().max(1);
                }
            }
            _ => {}
        }
    }

    fn apply_select(&mut self) {
        let Some(popup) = self.select.take() else {
            return;
        };
        let value = match popup.field {
            FieldKind::RegionZone(_) => (popup.selected + 1).to_string(),
            FieldKind::Quadrature => popup.options[popup.selected].clone(),
            _ => popup.options[popup.selected].clone(),
        };
        self.model.set_field(popup.field, &value);
        self.validation = self.model.validate();
        self.status = format!("{} = {}", popup.title, value);
        self.level = Level::Info;
    }

    /// Save the configuration to the next auto-numbered inputs path.
    pub fn save(&mut self) {
        if let Validation::Invalid(error) = self.model.validate() {
            self.validation = Validation::Invalid(error.clone());
            self.status = format!("Cannot save: {error}");
            self.level = Level::Error;
            return;
        }
        self.open_save_dialog_with(paths::next_input_path());
    }

    /// Open the save-as dialog with a default path.
    pub fn open_save_dialog_with(&mut self, default: PathBuf) {
        self.save_dialog = Some(PathDialog {
            purpose: DialogPurpose::SaveConfig,
            input: TextInput::new(&default.to_string_lossy()),
        });
        self.level = Level::Info;
    }

    /// Open the path dialog for the expansion output.
    pub fn open_output_dialog(&mut self) {
        let default = self.sweeps.output.clone();
        self.save_dialog = Some(PathDialog {
            purpose: DialogPurpose::ExpansionOutput,
            input: TextInput::new(&default.to_string_lossy()),
        });
        self.level = Level::Info;
    }

    /// Whether the save-as dialog is open.
    pub fn save_dialog_open(&self) -> bool {
        self.save_dialog.is_some()
    }

    /// Current path shown in the save-as dialog.
    pub fn save_dialog_value(&self) -> Option<&str> {
        self.save_dialog.as_ref().map(|dialog| dialog.input.text())
    }

    fn handle_save_dialog_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc => self.save_dialog = None,
            KeyCode::Enter => self.commit_save_dialog(),
            code => {
                if let Some(dialog) = self.save_dialog.as_mut() {
                    text_input_key(&mut dialog.input, code);
                }
            }
        }
    }

    fn commit_save_dialog(&mut self) {
        let Some(dialog) = self.save_dialog.clone() else {
            return;
        };
        let raw = dialog.input.text().to_string();
        match resolve_save_path(&raw) {
            Ok(path) => match dialog.purpose {
                DialogPurpose::SaveConfig => match self.save_to(&path) {
                    Ok(()) => {
                        self.save_dialog = None;
                        self.status = format!("Saved {}", path.display());
                        self.level = Level::Ok;
                        self.refresh_run_entries();
                        self.refresh_saved();
                    }
                    Err(error) => {
                        self.status = format!("Save failed: {error}");
                        self.level = Level::Error;
                        self.save_dialog = Some(dialog);
                    }
                },
                DialogPurpose::ExpansionOutput => {
                    self.sweeps.output = path.clone();
                    let saved = self.expand_placeholders_to(path);
                    self.save_dialog = if saved { None } else { Some(dialog) };
                }
            },
            Err(error) => {
                self.status = format!("Save failed: {error}");
                self.level = Level::Error;
                self.save_dialog = Some(dialog);
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

    fn edit_sweep_cell(&mut self) {
        let Some(range) = self.sweeps.ranges.get(self.sweeps.selected) else {
            return;
        };
        let (label, value) = match self.sweeps.column {
            0 => ("start", range.start.clone()),
            1 => ("end", range.end.clone()),
            _ => ("step", range.step.clone()),
        };
        self.editing = Some(Edit {
            target: EditTarget::Sweep {
                row: self.sweeps.selected,
                column: self.sweeps.column,
            },
            label: format!("range {} {label}", self.sweeps.selected + 1),
            input: TextInput::new(&value),
        });
    }

    /// Expand the current ranges over the selected configuration.
    pub fn expand_placeholders(&mut self) {
        let output = paths::outputs_dir().join("inputs").join("inputs_1d.json");
        self.expand_placeholders_to(output);
    }

    /// Expand the current ranges and write the list to an explicit path.
    pub fn expand_placeholders_to(&mut self, output: PathBuf) -> bool {
        let saved_index = self
            .sweeps
            .placeholder_entries
            .get(self.sweeps.config_selected)
            .copied()
            .unwrap_or(self.sweeps.config_selected);
        let Some(entry) = self.saved.get(saved_index).cloned() else {
            self.sweeps.message = "No source configuration (outputs/inputs is empty).".into();
            self.status = self.sweeps.message.clone();
            self.level = Level::Error;
            return false;
        };
        let value = match config_store::load_entry(&entry) {
            Ok(value) => value,
            Err(error) => {
                self.sweeps.message = format!("Could not read {}: {error}", entry.label());
                self.status = self.sweeps.message.clone();
                self.level = Level::Error;
                return false;
            }
        };

        match sweep::expand_strict(&[(entry.label(), value)], &self.sweeps.ranges) {
            Ok(expansion) if expansion.configs.is_empty() => {
                self.sweeps.message = format!(
                    "No configurations generated ({} problem(s)).",
                    expansion.errors.len()
                );
                self.status = self.sweeps.message.clone();
                self.level = Level::Error;
                false
            }
            Ok(expansion) => {
                let total = expansion.configs.len();
                match crate::cli::write_json(&output, &Value::Array(expansion.configs)) {
                    Ok(()) => {
                        self.sweeps.output = output.clone();
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
                        self.refresh_saved();
                        self.refresh_run_entries();
                        true
                    }
                    Err(error) => {
                        self.sweeps.message = format!("Write failed: {error}");
                        self.status = self.sweeps.message.clone();
                        self.level = Level::Error;
                        false
                    }
                }
            }
            Err(error) => {
                self.sweeps.message = format!("Expansion failed: {error}");
                self.status = self.sweeps.message.clone();
                self.level = Level::Error;
                false
            }
        }
    }

    /// Rebuild the list of configurations that contain placeholders.
    pub fn refresh_placeholder_configs(&mut self) {
        let previous = self
            .sweeps
            .placeholder_entries
            .get(self.sweeps.config_selected)
            .copied();
        let mut entries = Vec::new();
        let mut names = Vec::new();
        for (index, entry) in self.saved.iter().enumerate() {
            let Ok(value) = config_store::load_entry(entry) else {
                continue;
            };
            let detected: Vec<String> = crate::placeholder::detect_placeholders(&value)
                .into_iter()
                .collect();
            if !detected.is_empty() {
                entries.push(index);
                names.push(detected);
            }
        }
        self.sweeps.placeholder_entries = entries;
        self.sweeps.placeholder_names = names;
        self.sweeps.config_selected = previous
            .and_then(|previous| {
                self.sweeps
                    .placeholder_entries
                    .iter()
                    .position(|index| *index == previous)
            })
            .unwrap_or(0);
    }

    /// Detect placeholders in the selected source configuration.
    pub fn scan_placeholders(&mut self) {
        self.refresh_placeholder_configs();
        if self.sweeps.placeholder_entries.is_empty() {
            self.sweeps.ranges.clear();
            self.sweeps.message = "No configurations with placeholders found.".into();
            return;
        }
        self.load_placeholder_selection();
    }

    fn load_placeholder_selection(&mut self) {
        let Some(&saved_index) = self
            .sweeps
            .placeholder_entries
            .get(self.sweeps.config_selected)
        else {
            self.sweeps.ranges.clear();
            return;
        };
        let Some(entry) = self.saved.get(saved_index) else {
            self.sweeps.ranges.clear();
            return;
        };
        let names = self
            .sweeps
            .placeholder_names
            .get(self.sweeps.config_selected)
            .cloned()
            .unwrap_or_default();
        if names.is_empty() {
            self.sweeps.ranges.clear();
            self.sweeps.message = format!("No placeholders found in {}.", entry.label());
            return;
        }
        self.sweeps.ranges = names
            .into_iter()
            .map(crate::sweep::SweepRange::new)
            .collect();
        self.sweeps.selected = 0;
        self.sweeps.column = 0;
        self.sweeps.scroll = 0;
        self.sweeps.message = format!(
            "Detected {} placeholder(s) in {}.",
            self.sweeps.ranges.len(),
            entry.label()
        );
        self.level = Level::Info;
    }

    fn select_placeholder_config(&mut self, delta: i32) {
        if self.sweeps.placeholder_entries.is_empty() {
            return;
        }
        let count = self.sweeps.placeholder_entries.len() as i32;
        self.sweeps.config_selected =
            (self.sweeps.config_selected as i32 + delta).rem_euclid(count) as usize;
        self.load_placeholder_selection();
    }

    /// Expand and run every generated configuration.
    pub fn run_batch(&mut self) {
        let list = self.sweeps.output.clone();
        let output = paths::next_output_path();
        self.run_batch_to(list, output);
    }

    /// Expand to `list_path` and run the resulting configurations.
    pub fn run_batch_to(&mut self, list_path: PathBuf, output: PathBuf) {
        if self.run.running {
            self.status = "A run is already in progress.".into();
            self.level = Level::Info;
            return;
        }
        if !self.expand_placeholders_to(list_path.clone()) {
            return;
        }
        let value = match config_store::load(&list_path) {
            Ok(value) => value,
            Err(error) => {
                self.status = format!("Could not read {}: {error}", list_path.display());
                self.level = Level::Error;
                return;
            }
        };
        let Some(items) = value.as_array() else {
            self.status = "Expanded output is not a list.".into();
            self.level = Level::Error;
            return;
        };
        let mut configs = Vec::with_capacity(items.len());
        for (index, item) in items.iter().enumerate() {
            match Config::from_value_with(item, RunOverrides::default(), None) {
                Ok(config) => configs.push((
                    format!("json_config_{:03}", index + 1),
                    item.clone(),
                    config,
                )),
                Err(error) => {
                    self.status = format!("Config {}: {error}", index + 1);
                    self.level = Level::Error;
                    return;
                }
            }
        }
        if configs.is_empty() {
            self.status = "No configurations to run.".into();
            self.level = Level::Error;
            return;
        }

        let count = configs.len();
        self.run.log.clear();
        self.run.log_offset = 0;
        self.run.log.push(format!("batch: {count} config(s)"));
        self.run.current = Some(format!("batch ({count} configs)"));
        self.run.output = Some(output.clone());
        self.run.running = true;
        self.run.handle = Some(runner::spawn_batch(configs, output));
        self.status = format!("Batch started ({count} configs).");
        self.level = Level::Info;
        self.switch_tab(Tab::Run);
    }

    /// Reload the saved-configuration list.
    pub fn refresh_saved(&mut self) {
        self.saved = config_store::list();
        if self.saved_selected >= self.saved.len() {
            self.saved_selected = self.saved.len().saturating_sub(1);
        }
        if self.sweeps.config_selected >= self.saved.len() {
            self.sweeps.config_selected = self.saved.len().saturating_sub(1);
        }
    }

    /// Load a saved configuration (or list item) into the form.
    pub fn load_saved(&mut self, index: usize) {
        let Some(entry) = self.saved.get(index).cloned() else {
            return;
        };
        match config_store::load_entry(&entry).and_then(|value| ConfigModel::from_value(&value)) {
            Ok(model) => {
                self.model = model;
                self.selection = Selection::Field(FieldKind::NumRegions);
                self.validation = self.model.validate();
                self.saved_selected = index;
                self.config_focus = ConfigFocus::Form;
                self.status = format!("Loaded {}", entry.label());
                self.level = Level::Ok;
            }
            Err(error) => {
                self.status = format!("Could not load {}: {error}", entry.label());
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
        let value = edit.input.text().to_string();
        match edit.target {
            EditTarget::Field(kind) => {
                self.model.set_field(kind, &value);
                self.validation = self.model.validate();
            }
            EditTarget::Sweep { row, column } => {
                if let Some(range) = self.sweeps.ranges.get_mut(row) {
                    match column {
                        0 => range.start = value.trim().to_string(),
                        1 => range.end = value.trim().to_string(),
                        _ => range.step = value.trim().to_string(),
                    }
                }
                self.sweeps.message.clear();
            }
        }
        self.status = format!("{} = {value}", edit.label);
        self.level = Level::Info;
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
        let value = match config_store::load_entry(&entry) {
            Ok(value) => value,
            Err(error) => {
                self.status = format!("Could not read {}: {error}", entry.label());
                self.level = Level::Error;
                return;
            }
        };
        let placeholders = crate::placeholder::detect_placeholders(&value);
        if !placeholders.is_empty() {
            self.status = format!(
                "{} has placeholders ({}); expand it in the Placeholders tab.",
                entry.label(),
                placeholders.into_iter().collect::<Vec<_>>().join(", ")
            );
            self.level = Level::Error;
            return;
        }
        let config = match Config::from_value_with(&value, RunOverrides::default(), None) {
            Ok(config) => config,
            Err(error) => {
                self.status = format!("{}: {error}", entry.name);
                self.level = Level::Error;
                return;
            }
        };

        let name = entry.label();
        self.run.log.clear();
        self.run.log_offset = 0;
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
                RunEvent::BatchDone {
                    output,
                    total,
                    failures,
                    cancelled,
                } => {
                    self.run.running = false;
                    self.run.handle = None;
                    let note = if cancelled { "cancelled" } else { "finished" };
                    self.run.log.push(format!(
                        "batch {note}: {total} run(s), {failures} failure(s)"
                    ));
                    self.run.output = Some(output.clone());
                    self.status = format!("Batch {note}: {} ({total} run(s))", output.display());
                    self.level = if failures > 0 {
                        Level::Error
                    } else {
                        Level::Ok
                    };
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

    fn rows(&self) -> Vec<FormRow> {
        let fields = self.model.fields();
        let mut rows = Vec::new();
        for section in SectionId::ALL {
            let collapsed = self.collapsed_sections.contains(&section);
            let title = if section == SectionId::Materials {
                format!("Materials ({} zones)", self.model.num_zones)
            } else {
                section.title().to_string()
            };
            rows.push(FormRow {
                selection: Selection::Section(section),
                label: format!("{} {title}", if collapsed { "▶" } else { "▼" }),
                value: String::new(),
                header: true,
                depth: 0,
            });
            if collapsed {
                continue;
            }
            if section == SectionId::Materials {
                for zone in 0..self.model.num_zones {
                    let zone_collapsed = self.collapsed_zones.contains(&zone);
                    let name = self
                        .model
                        .zone_names
                        .get(zone)
                        .filter(|name| !name.is_empty())
                        .cloned()
                        .unwrap_or_else(|| format!("zone {}", zone + 1));
                    rows.push(FormRow {
                        selection: Selection::Zone(zone),
                        label: format!(
                            "{} Zone {} · {}",
                            if zone_collapsed { "▶" } else { "▼" },
                            zone + 1,
                            name
                        ),
                        value: String::new(),
                        header: true,
                        depth: 1,
                    });
                    if zone_collapsed {
                        continue;
                    }
                    for field in fields
                        .iter()
                        .filter(|field| zone_index(field.kind) == Some(zone))
                    {
                        rows.push(field_row(field, 2));
                    }
                }
            } else {
                for field in fields
                    .iter()
                    .filter(|field| SectionId::from_label(field.section) == section)
                {
                    rows.push(field_row(field, 1));
                }
            }
        }
        rows
    }

    /// Draw the whole interface.
    pub fn render(&mut self, frame: &mut Frame) {
        self.hitmap = HitMap::default();
        let area = frame.area();
        if area.width < 80 || area.height < 24 {
            let message = Paragraph::new(format!(
                "Terminal too small ({}x{}).\nMinimum: 80x24.",
                area.width, area.height
            ))
            .alignment(Alignment::Center)
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
        if self.select.is_some() {
            self.render_popup(frame);
        }
        if self.save_dialog.is_some() {
            self.render_save_dialog(frame);
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

    fn render_tabs(&mut self, frame: &mut Frame, area: Rect) {
        let mut x = area.x;
        for tab in Tab::ALL {
            if x >= area.x + area.width {
                break;
            }
            let width = (tab.title().len() as u16 + 3).min(area.x + area.width - x);
            let rect = Rect::new(x, area.y, width, 1);
            let style = if tab == self.tab {
                Style::default()
                    .bg(Color::Blue)
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::Gray)
            };
            frame.render_widget(
                Paragraph::new(tab.title())
                    .alignment(Alignment::Center)
                    .style(style),
                rect,
            );
            self.hitmap.actions.push((rect, Action::Tab(tab)));
            x += width;
        }
    }

    fn render_config(&mut self, frame: &mut Frame, area: Rect) {
        let columns = Layout::horizontal([Constraint::Percentage(62), Constraint::Percentage(38)])
            .split(area);
        self.render_form(frame, columns[0]);
        self.render_side_panel(frame, columns[1]);
    }

    fn render_form(&mut self, frame: &mut Frame, area: Rect) {
        let rows = self.rows();
        let selected = rows.iter().position(|row| row.selection == self.selection);
        self.table_state.select(selected);

        let table_rows = rows
            .iter()
            .enumerate()
            .map(|(index, row)| {
                let highlighted = index == selected.unwrap_or(usize::MAX);
                let indent = "  ".repeat(row.depth);
                let style = if row.header {
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default()
                };
                let style = if highlighted {
                    style.bg(Color::DarkGray)
                } else {
                    style
                };
                Row::new(vec![
                    Cell::from(format!("{indent}{}", row.label)),
                    Cell::from(row.value.clone()),
                ])
                .style(style)
            })
            .collect::<Vec<_>>();

        let table = Table::new(table_rows, [Constraint::Length(40), Constraint::Min(0)])
            .header(
                Row::new(vec!["Parameter", "Value"]).style(
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD),
                ),
            )
            .block(Block::bordered().title("Configuration"));
        frame.render_stateful_widget(table, area, &mut self.table_state);

        // Register row hotspots using the table offset chosen while drawing.
        let inner = Block::bordered().inner(area);
        let offset = self.table_state.offset();
        let first_y = inner.y + 1; // header row
        for (index, _) in rows.iter().enumerate().skip(offset) {
            let y = first_y + (index - offset) as u16;
            if y >= inner.y + inner.height {
                break;
            }
            let rect = Rect::new(inner.x, y, inner.width, 1);
            self.hitmap.actions.push((rect, Action::FormRow(index)));
        }
        self.hitmap.panels.push((area, Panel::Form));
    }

    fn render_side_panel(&mut self, frame: &mut Frame, area: Rect) {
        let chunks = Layout::vertical([
            Constraint::Length(7),
            Constraint::Length(5),
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
                format!(
                    "⚠ Template\nplaceholders: {}\nExpand in Placeholders (Tab).",
                    names.join(", ")
                ),
                Color::Yellow,
            ),
            Validation::Invalid(error) => (format!("✗ Invalid\n{error}"), Color::Red),
        };
        let status = Paragraph::new(text)
            .style(Style::default().fg(color))
            .wrap(Wrap { trim: true })
            .block(Block::bordered().title("Validation"));
        frame.render_widget(status, chunks[0]);

        let geometry = Paragraph::new(geometry_line(
            &self.model,
            chunks[1].width.saturating_sub(2) as usize,
        ))
        .wrap(Wrap { trim: true })
        .block(Block::bordered().title("Geometry"));
        frame.render_widget(geometry, chunks[1]);

        let tips = Paragraph::new(
            "kk/mm = placeholders → Placeholders\n\
             z/Z collapse · e edit · m mouse\n\
             s save as · v validate · L load config",
        )
        .style(Style::default().fg(Color::DarkGray))
        .wrap(Wrap { trim: true })
        .block(Block::bordered().title("Tips"));
        frame.render_widget(tips, chunks[2]);

        self.render_saved(frame, chunks[3]);
    }

    fn render_saved(&mut self, frame: &mut Frame, area: Rect) {
        let offset = self.saved_selected.saturating_sub(4);
        let lines: Vec<Line> = self
            .saved
            .iter()
            .enumerate()
            .skip(offset)
            .map(|(index, entry)| {
                let selected = index == self.saved_selected;
                let style = if selected {
                    if self.config_focus == ConfigFocus::Saved {
                        Style::default()
                            .bg(Color::DarkGray)
                            .fg(Color::Yellow)
                            .add_modifier(Modifier::BOLD)
                    } else {
                        Style::default().bg(Color::DarkGray)
                    }
                } else {
                    Style::default()
                };
                Line::from(Span::styled(
                    format!("{} {}", if selected { "▶" } else { " " }, entry.label()),
                    style,
                ))
            })
            .collect();
        let list = Paragraph::new(lines)
            .block(Block::bordered().title(format!("Configs ({}). L to browse", self.saved.len())))
            .scroll((offset as u16, 0));
        frame.render_widget(list, area);

        let inner = Block::bordered().inner(area);
        for visible in 0..inner.height as usize {
            let index = offset + visible;
            if index >= self.saved.len() {
                break;
            }
            self.hitmap.actions.push((
                Rect::new(inner.x, inner.y + visible as u16, inner.width, 1),
                Action::LoadSaved(index),
            ));
        }
        self.hitmap.panels.push((area, Panel::Saved));
    }

    fn render_run(&mut self, frame: &mut Frame, area: Rect) {
        let sections = Layout::vertical([Constraint::Length(3), Constraint::Min(0)]).split(area);
        self.render_buttons(
            frame,
            sections[0],
            &[("Run", Action::Run), ("Cancel", Action::Cancel)],
        );

        let columns = Layout::horizontal([Constraint::Percentage(36), Constraint::Percentage(64)])
            .split(sections[1]);

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
                    format!("{} {}", if selected { "▶" } else { " " }, entry.label()),
                    style,
                ))
            })
            .collect();
        let list = Paragraph::new(lines)
            .block(Block::bordered().title(format!("Configs ({})", self.run.entries.len())))
            .scroll((offset, 0));
        frame.render_widget(list, columns[0]);
        self.hitmap.panels.push((columns[0], Panel::RunList));
        let inner = Block::bordered().inner(columns[0]);
        for index in self.run.selected.saturating_sub(8)
            ..self
                .run
                .entries
                .len()
                .min(self.run.selected.saturating_sub(8) + inner.height as usize)
        {
            let y = inner.y + (index - self.run.selected.saturating_sub(8)) as u16;
            self.hitmap.actions.push((
                Rect::new(inner.x, y, inner.width, 1),
                Action::RunList(index),
            ));
        }

        let right = Layout::vertical([Constraint::Length(3), Constraint::Min(0)]).split(columns[1]);
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
        frame.render_widget(header, right[0]);

        let height = right[1].height.saturating_sub(2) as usize;
        let end = self.run.log.len().saturating_sub(self.run.log_offset);
        let start = end.saturating_sub(height);
        let log_lines: Vec<Line> = self.run.log[start..end]
            .iter()
            .map(|line| Line::from(line.clone()))
            .collect();
        let log = Paragraph::new(log_lines)
            .wrap(Wrap { trim: false })
            .block(Block::bordered().title("Log"));
        frame.render_widget(log, right[1]);
        self.hitmap.panels.push((right[1], Panel::RunLog));
    }

    fn render_results(&mut self, frame: &mut Frame, area: Rect) {
        let sections = Layout::vertical([Constraint::Length(6), Constraint::Min(0)]).split(area);

        let offset = self.results.selected.saturating_sub(4) as u16;
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
        self.hitmap.panels.push((sections[0], Panel::ResultsList));
        let inner = Block::bordered().inner(sections[0]);
        for index in self.results.selected.saturating_sub(4)
            ..self
                .results
                .entries
                .len()
                .min(self.results.selected.saturating_sub(4) + inner.height as usize)
        {
            let y = inner.y + (index - self.results.selected.saturating_sub(4)) as u16;
            self.hitmap.actions.push((
                Rect::new(inner.x, y, inner.width, 1),
                Action::ResultList(index),
            ));
        }

        match &self.results.payload {
            Some(payload) => {
                let runs = payload
                    .get("runs")
                    .and_then(Value::as_array)
                    .cloned()
                    .unwrap_or_default();
                let rows = runs
                    .iter()
                    .skip(self.results.table_offset)
                    .map(|run| {
                        Row::new(vec![
                            Cell::from(
                                run.get("config_name")
                                    .and_then(Value::as_str)
                                    .unwrap_or("?"),
                            ),
                            Cell::from(run.get("mode").and_then(Value::as_str).unwrap_or("-")),
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
                    .collect::<Vec<_>>();
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
                self.hitmap.panels.push((sections[1], Panel::ResultsTable));
            }
            None => {
                let message =
                    Paragraph::new("No output selected.").block(Block::bordered().title("Runs"));
                frame.render_widget(message, sections[1]);
                self.hitmap.panels.push((sections[1], Panel::ResultsTable));
            }
        }
    }

    fn render_sweeps(&mut self, frame: &mut Frame, area: Rect) {
        let sections = Layout::vertical([
            Constraint::Length(3),
            Constraint::Min(0),
            Constraint::Length(7),
            Constraint::Length(5),
        ])
        .split(area);
        self.render_buttons(
            frame,
            sections[0],
            &[
                ("Save", Action::SaveExpansion),
                ("Run batch", Action::RunBatch),
                ("Output", Action::EditOutput),
            ],
        );

        let offset = self.sweeps.scroll;
        let rows = self
            .sweeps
            .ranges
            .iter()
            .skip(offset)
            .enumerate()
            .map(|(visible, range)| {
                let index = offset + visible;
                let selected = index == self.sweeps.selected;
                let style = if selected {
                    Style::default()
                        .bg(Color::DarkGray)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default()
                };
                let value_cell = |edit_column: usize, text: &str| {
                    if selected && edit_column == self.sweeps.column {
                        Cell::from(Span::styled(
                            text.to_string(),
                            Style::default().fg(Color::Yellow),
                        ))
                    } else {
                        Cell::from(text.to_string())
                    }
                };
                let values =
                    range_values(range).map_or_else(|| "!".to_string(), |count| count.to_string());
                Row::new(vec![
                    Cell::from(range.name.clone()),
                    value_cell(0, &range.start),
                    value_cell(1, &range.end),
                    value_cell(2, &range.step),
                    Cell::from(values),
                ])
                .style(style)
            })
            .collect::<Vec<_>>();

        let table = Table::new(
            rows,
            [
                Constraint::Length(14),
                Constraint::Length(10),
                Constraint::Length(10),
                Constraint::Length(10),
                Constraint::Length(8),
            ],
        )
        .header(Row::new(vec!["Name", "Start", "End", "Step", "Values"]))
        .block(Block::bordered().title("Placeholders"));
        frame.render_widget(table, sections[1]);

        let inner = Block::bordered().inner(sections[1]);
        for visible in 0..inner.height as usize {
            let index = offset + visible;
            if index >= self.sweeps.ranges.len() {
                break;
            }
            self.hitmap.actions.push((
                Rect::new(inner.x, inner.y + 1 + visible as u16, inner.width, 1),
                Action::SweepRow(index),
            ));
        }
        self.hitmap.panels.push((sections[1], Panel::SweepTable));

        self.render_placeholder_configs(frame, sections[2]);

        let config_label = self
            .sweeps
            .placeholder_entries
            .get(self.sweeps.config_selected)
            .and_then(|index| self.saved.get(*index))
            .map_or_else(|| "<none>".to_string(), |entry| entry.label());
        let counts: Vec<Option<usize>> = self.sweeps.ranges.iter().map(range_values).collect();
        let values_line = if counts.is_empty() {
            "values: -".to_string()
        } else if counts.iter().all(|count| *count == counts[0]) {
            format!("values: {} per placeholder", counts[0].unwrap_or(0))
        } else {
            "values: lengths differ (must be equal)".to_string()
        };
        let info = Paragraph::new(vec![
            Line::from(self.sweeps.message.clone()),
            Line::from(format!("config: {config_label}")),
            Line::from(format!("output: {}", self.sweeps.output.display())),
            Line::from(Span::styled(
                values_line,
                Style::default().fg(Color::DarkGray),
            )),
        ])
        .wrap(Wrap { trim: true })
        .block(Block::bordered().title("Expansion"));
        frame.render_widget(info, sections[3]);
    }

    fn render_placeholder_configs(&mut self, frame: &mut Frame, area: Rect) {
        let offset = self.sweeps.config_selected.saturating_sub(4);
        let lines: Vec<Line> = self
            .sweeps
            .placeholder_entries
            .iter()
            .enumerate()
            .skip(offset)
            .map(|(index, saved_index)| {
                let entry = &self.saved[*saved_index];
                let names = self
                    .sweeps
                    .placeholder_names
                    .get(index)
                    .cloned()
                    .unwrap_or_default();
                let selected = index == self.sweeps.config_selected;
                let style = if selected {
                    Style::default()
                        .bg(Color::DarkGray)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default()
                };
                Line::from(Span::styled(
                    format!(
                        "{} {} [{}]",
                        if selected { "▶" } else { " " },
                        entry.label(),
                        names.join(", ")
                    ),
                    style,
                ))
            })
            .collect();
        let list = Paragraph::new(lines)
            .block(Block::bordered().title(format!(
                "Placeholder configs ({})",
                self.sweeps.placeholder_entries.len()
            )))
            .scroll((offset as u16, 0));
        frame.render_widget(list, area);

        let inner = Block::bordered().inner(area);
        for visible in 0..inner.height as usize {
            let index = offset + visible;
            if index >= self.sweeps.placeholder_entries.len() {
                break;
            }
            self.hitmap.actions.push((
                Rect::new(inner.x, inner.y + visible as u16, inner.width, 1),
                Action::SelectPlaceholder(index),
            ));
        }
        self.hitmap.panels.push((area, Panel::PlaceholderConfigs));
    }

    fn render_plots(&mut self, frame: &mut Frame, area: Rect) {
        let sections = Layout::vertical([Constraint::Length(3), Constraint::Min(0)]).split(area);
        self.render_buttons(
            frame,
            sections[0],
            &[
                ("Kind", Action::NextKind),
                ("Generate", Action::GeneratePlot),
                ("Open", Action::OpenPlot),
            ],
        );

        let columns = Layout::horizontal([Constraint::Percentage(42), Constraint::Percentage(58)])
            .split(sections[1]);
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
        self.hitmap.panels.push((columns[0], Panel::PlotInfo));

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

    fn render_export(&mut self, frame: &mut Frame, area: Rect) {
        let sections = Layout::vertical([Constraint::Length(3), Constraint::Min(0)]).split(area);
        self.render_buttons(
            frame,
            sections[0],
            &[
                ("Data", Action::ExportData),
                ("Excel", Action::ExportExcel),
                ("SCS", Action::ExportScs),
                ("Open", Action::OpenExport),
            ],
        );

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
        frame.render_widget(info, sections[1]);
        self.hitmap.panels.push((sections[1], Panel::ExportInfo));
    }

    fn render_buttons(&mut self, frame: &mut Frame, area: Rect, buttons: &[(&str, Action)]) {
        let constraints: Vec<Constraint> = buttons
            .iter()
            .map(|(label, _)| Constraint::Length(label.len() as u16 + 4))
            .collect();
        let chunks = Layout::horizontal(constraints).split(area);
        for ((label, action), rect) in buttons.iter().zip(chunks.iter()) {
            let block = Block::bordered().border_style(Style::default().fg(Color::Gray));
            let paragraph = Paragraph::new(*label)
                .alignment(Alignment::Center)
                .block(block);
            frame.render_widget(paragraph, *rect);
            self.hitmap.actions.push((*rect, *action));
        }
    }

    fn render_footer(&self, frame: &mut Frame, area: Rect) {
        let help = match self.tab {
            Tab::Config => {
                "↑/↓ move   ←/→ fold/cycle   Enter open/toggle   e edit   z/Z collapse   s save as   Tab switch"
            }
            Tab::Run => "↑/↓ select   r run   c cancel   Tab switch   q quit",
            Tab::Results => "↑/↓ select output   wheel scroll   Tab switch   q quit",
            Tab::Placeholders => {
                "[ ]/click config   ↑/↓ row   ←/→ column   Enter edit   x save   R run   r rescan"
            }
            Tab::Plots => "k/←/→ kind   g generate   o open   Tab switch   q quit",
            Tab::Export => "d data   x excel   s scs plot   o open   Tab switch   q quit",
        };
        let level_style = match self.level {
            Level::Info => Style::default().fg(Color::Gray),
            Level::Ok => Style::default().fg(Color::Green),
            Level::Error => Style::default().fg(Color::Red),
        };
        let mouse = if self.mouse_captured {
            "mouse:on"
        } else {
            "mouse:off"
        };
        let lines = vec![
            Line::from(Span::styled(
                format!("{help}   m {mouse}   q quit"),
                Style::default().fg(Color::DarkGray),
            )),
            Line::from(Span::styled(self.status.clone(), level_style)),
        ];
        frame.render_widget(Paragraph::new(lines), area);
    }

    fn render_editor(&self, frame: &mut Frame) {
        let Some(edit) = &self.editing else {
            return;
        };
        let width = 22u16.min(frame.area().width.saturating_sub(4)).max(16);
        let area = centered_rect_width(frame.area(), width, 3);
        frame.render_widget(Clear, area);
        let block = Block::bordered().title(truncate_label(
            &edit.label,
            width.saturating_sub(2) as usize,
        ));
        let (visible, column) = cursor_window(
            edit.input.text(),
            edit.input.cursor,
            width.saturating_sub(4) as usize,
        );
        let inner = block.inner(area);
        frame.render_widget(Paragraph::new(visible).block(block), area);
        frame.set_cursor_position((inner.x + column as u16, inner.y));
    }

    fn render_popup(&mut self, frame: &mut Frame) {
        let Some(popup) = self.select.clone() else {
            return;
        };
        let height = (popup.options.len() as u16 + 2).min(14);
        let area = centered_rect(frame.area(), 36, height);
        frame.render_widget(Clear, area);
        let items: Vec<ListItem> = popup
            .options
            .iter()
            .map(|option| ListItem::new(option.clone()))
            .collect();
        let list = List::new(items)
            .block(Block::bordered().title(popup.title.clone()))
            .highlight_style(
                Style::default()
                    .bg(Color::Blue)
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD),
            )
            .highlight_symbol("▶ ");
        let mut state = ratatui::widgets::ListState::default().with_selected(Some(popup.selected));
        frame.render_stateful_widget(list, area, &mut state);

        let inner = Block::bordered().inner(area);
        for (index, _) in popup.options.iter().enumerate() {
            if index < inner.height as usize {
                self.hitmap.actions.push((
                    Rect::new(inner.x, inner.y + index as u16, inner.width, 1),
                    Action::SelectOption(index),
                ));
            }
        }
        self.hitmap.panels.push((area, Panel::PlotInfo));
    }

    fn render_save_dialog(&self, frame: &mut Frame) {
        let Some(dialog) = self.save_dialog.as_ref() else {
            return;
        };
        let width = 76u16.min(frame.area().width.saturating_sub(4)).max(30);
        let area = centered_rect_width(frame.area(), width, 5);
        frame.render_widget(Clear, area);
        let (visible, column) = cursor_window(
            dialog.input.text(),
            dialog.input.cursor,
            width.saturating_sub(4) as usize,
        );
        let title = match dialog.purpose {
            DialogPurpose::SaveConfig => "Save configuration",
            DialogPurpose::ExpansionOutput => "Expansion output",
        };
        let lines = vec![
            Line::from(visible),
            Line::from(""),
            Line::from(Span::styled(
                "Enter save   Esc cancel   relative paths use NTS_HOME",
                Style::default().fg(Color::DarkGray),
            )),
        ];
        let block = Block::bordered().title(title);
        let inner = block.inner(area);
        frame.render_widget(Paragraph::new(lines).block(block), area);
        frame.set_cursor_position((inner.x + column as u16, inner.y));
    }
}

fn resolve_save_path(raw: &str) -> Result<PathBuf, String> {
    let text = raw.trim();
    if text.is_empty() {
        return Err("path cannot be empty".to_string());
    }

    let expanded = if let Some(rest) = text.strip_prefix("~/") {
        match std::env::var_os("HOME") {
            Some(home) => PathBuf::from(home).join(rest),
            None => PathBuf::from(text),
        }
    } else {
        PathBuf::from(text)
    };

    let path = if expanded.is_absolute() {
        expanded
    } else if expanded.components().count() == 1 {
        paths::outputs_dir().join("inputs").join(expanded)
    } else {
        paths::work_root().join(expanded)
    };

    match path.extension().and_then(|extension| extension.to_str()) {
        None => Ok(path.with_extension("json")),
        Some("json") => Ok(path),
        Some(_) => Err("path must end with .json".to_string()),
    }
}

fn field_row(field: &crate::tui::model::Field, depth: usize) -> FormRow {
    FormRow {
        selection: Selection::Field(field.kind),
        label: field.label.clone(),
        value: field.value.clone(),
        header: false,
        depth,
    }
}

fn range_values(range: &SweepRange) -> Option<usize> {
    let start = range.start.trim().parse::<f64>().ok()?;
    let end = range.end.trim().parse::<f64>().ok()?;
    let step = range.step.trim().parse::<f64>().ok()?;
    crate::placeholder::build_values(start, end, step)
        .ok()
        .map(|values| values.len())
}

fn zone_index(kind: FieldKind) -> Option<usize> {
    match kind {
        FieldKind::ZoneSigmaT(index)
        | FieldKind::ZoneSigmaS(index)
        | FieldKind::ZoneNuSigf(index)
        | FieldKind::ZoneSigmaF(index) => Some(index),
        _ => None,
    }
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

const ZONE_COLORS: [Color; 8] = [
    Color::Cyan,
    Color::Green,
    Color::Yellow,
    Color::Magenta,
    Color::LightBlue,
    Color::LightGreen,
    Color::LightRed,
    Color::LightMagenta,
];

fn zone_color(zone: usize) -> Color {
    ZONE_COLORS[(zone.saturating_sub(1)) % ZONE_COLORS.len()]
}

fn geometry_line(model: &ConfigModel, available: usize) -> Line<'static> {
    let segments: Vec<(f64, usize)> = model
        .hr
        .iter()
        .enumerate()
        .filter_map(|(index, value)| {
            let width = value.trim().parse::<f64>().ok()?;
            if width <= 0.0 {
                return None;
            }
            let zone = model
                .izl
                .get(index)
                .and_then(|value| value.trim().parse::<usize>().ok())
                .unwrap_or(index + 1);
            Some((width, zone))
        })
        .collect();

    if segments.is_empty() {
        return Line::from("[no geometry]");
    }

    let total: f64 = segments.iter().map(|(width, _)| width).sum();
    let label = format!("{total:.2} cm");
    let separators = segments.len().saturating_sub(1);
    let slots = available
        .saturating_sub(label.len() + 3 + separators)
        .max(4);

    let mut spans: Vec<Span<'static>> = vec![Span::raw("[")];
    for (index, (width, zone)) in segments.iter().enumerate() {
        if index > 0 {
            spans.push(Span::styled("│", Style::default().fg(Color::DarkGray)));
        }
        let cells = ((width / total) * slots as f64).round().max(1.0) as usize;
        spans.push(Span::styled(
            "█".repeat(cells),
            Style::default().fg(zone_color(*zone)),
        ));
    }
    spans.push(Span::raw("] "));
    spans.push(Span::raw(label));
    Line::from(spans)
}

fn centered_rect_width(area: Rect, width: u16, height: u16) -> Rect {
    let horizontal = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Fill(1),
            Constraint::Length(width.min(area.width)),
            Constraint::Fill(1),
        ])
        .split(area);
    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Fill(1),
            Constraint::Length(height.min(area.height)),
            Constraint::Fill(1),
        ])
        .split(horizontal[1]);
    vertical[1]
}

fn cursor_window(text: &str, cursor: usize, max_cells: usize) -> (String, usize) {
    let characters: Vec<char> = text.chars().collect();
    let length = characters.len();
    let cursor = cursor.min(length);
    let max_cells = max_cells.max(2);
    // Up to two ellipsis cells are reserved; the cursor is drawn by the
    // terminal, so it does not consume a cell or split a number.
    let budget = max_cells.saturating_sub(2).max(1);
    let (start, end) = if length <= budget {
        (0, length)
    } else {
        let mut start = cursor.saturating_sub(budget / 2);
        if start + budget > length {
            start = length - budget;
        }
        (start, start + budget)
    };

    let mut out = String::new();
    let left_ellipsis = start > 0;
    if left_ellipsis {
        out.push('…');
    }
    out.extend(characters[start..end].iter());
    if end < length {
        out.push('…');
    }
    let column = usize::from(left_ellipsis) + (cursor - start).min(end - start);
    (out, column)
}

fn truncate_label(text: &str, max_chars: usize) -> String {
    let characters: Vec<char> = text.chars().collect();
    if characters.len() <= max_chars {
        return text.to_string();
    }
    let keep = max_chars.saturating_sub(1);
    let mut out: String = characters[..keep].iter().collect();
    out.push('…');
    out
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
        let line = geometry_line(&model, 50);
        let text: String = line
            .spans
            .iter()
            .map(|span| span.content.as_ref())
            .collect();
        assert!(text.starts_with('['));
        assert!(text.contains('█'));
        assert!(text.contains("20.00 cm"));
        assert!(text.chars().count() <= 50);
    }

    #[test]
    fn geometry_bar_uses_one_colour_per_zone() {
        let model = ConfigModel {
            num_regions: 2,
            nc: vec!["5".into(), "5".into()],
            hr: vec!["5".into(), "5".into()],
            izl: vec!["1".into(), "2".into()],
            ..ConfigModel::default()
        };

        let line = geometry_line(&model, 60);
        let colors: Vec<Color> = line
            .spans
            .iter()
            .filter(|span| span.content.contains('█'))
            .filter_map(|span| span.style.fg)
            .collect();
        assert!(colors.len() >= 2);
        assert_ne!(colors[0], colors[1]);
        assert_eq!(zone_color(1), zone_color(1));
        assert_ne!(zone_color(1), zone_color(2));
    }

    #[test]
    fn rows_respect_collapsed_sections() {
        let mut app = App::default();
        let before = app.rows().len();
        app.collapsed_sections.insert(SectionId::Geometry);
        let after = app.rows().len();
        assert!(after < before);
        assert!(
            app.rows()
                .iter()
                .any(|row| row.selection == Selection::Section(SectionId::Geometry))
        );
    }

    #[test]
    fn rows_group_zones_under_materials() {
        let mut app = App::default();
        app.model.num_zones = 2;
        app.model.sct.push("1.0".into());
        app.model.scs.push("0.9".into());
        app.model.nu_sigf.push("0.0".into());
        app.model.sigma_f.push(String::new());

        let rows = app.rows();
        assert!(rows.iter().any(|row| row.selection == Selection::Zone(0)));
        assert!(rows.iter().any(|row| row.selection == Selection::Zone(1)));

        app.collapsed_zones.insert(0);
        let rows = app.rows();
        assert!(
            !rows
                .iter()
                .any(|row| row.selection == Selection::Field(FieldKind::ZoneSigmaT(0)))
        );
        assert!(
            rows.iter()
                .any(|row| row.selection == Selection::Field(FieldKind::ZoneSigmaT(1)))
        );
    }

    #[test]
    fn save_paths_are_resolved() {
        let absolute = std::env::temp_dir().join("case.json");
        assert_eq!(
            resolve_save_path(absolute.to_str().unwrap()).unwrap(),
            absolute
        );

        let no_extension = std::env::temp_dir().join("case");
        assert_eq!(
            resolve_save_path(no_extension.to_str().unwrap()).unwrap(),
            absolute
        );

        assert!(
            resolve_save_path(std::env::temp_dir().join("case.txt").to_str().unwrap()).is_err()
        );
        assert!(resolve_save_path("").is_err());

        let bare = resolve_save_path("bare.json").unwrap();
        assert!(bare.ends_with("outputs/inputs/bare.json"));
    }

    #[test]
    fn text_input_edits_at_the_cursor() {
        let mut input = TextInput::new("20");
        input.home();
        input.insert('1');
        assert_eq!(input.text(), "120");

        input.move_right();
        input.insert('5');
        assert_eq!(input.text(), "1250");

        input.end();
        input.backspace();
        assert_eq!(input.text(), "125");
        input.home();
        input.delete();
        assert_eq!(input.text(), "25");

        input.move_left();
        assert_eq!(input.cursor, 0);
    }

    #[test]
    fn cursor_window_keeps_the_cursor_visible() {
        let (short, column) = cursor_window("abc", 2, 10);
        assert_eq!(short, "abc");
        assert_eq!(column, 2);

        let long = "abcdefghijklmnopqrstuvwxyz";
        let (window, column) = cursor_window(long, 20, 10);
        assert!(!window.contains('▏'));
        assert!(window.starts_with('…'));
        assert!(window.chars().count() <= 10);
        assert!(column < 10);

        let (at_end, column) = cursor_window("abcdef", 6, 5);
        assert!(at_end.starts_with('…'));
        assert_eq!(column, at_end.chars().count());
    }

    #[test]
    fn editor_stays_narrow() {
        let area = Rect::new(0, 0, 120, 40);
        let editor = centered_rect_width(area, 22, 3);
        assert_eq!(editor.width, 22);
        assert_eq!(editor.height, 3);
        assert_eq!(editor.x, (120 - 22) / 2);
    }

    #[test]
    fn default_selection_is_a_field() {
        let app = App::default();
        assert_eq!(app.selection, Selection::Field(FieldKind::NumRegions));
    }

    #[test]
    fn popup_targets_are_derived_from_the_model() {
        let mut app = App::default();
        app.open_select(FieldKind::Mode);
        assert_eq!(app.select.as_ref().unwrap().options.len(), 4);
        assert_eq!(app.select.as_ref().unwrap().selected, 0);
        app.select.as_mut().unwrap().selected = 3;
        app.apply_select();
        assert_eq!(app.model.mode, 3);
    }
}
