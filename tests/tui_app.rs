//! TUI phase A: config screen rendering, editing, validation and saving.

use neutron_transport_dd::config::Config;
use neutron_transport_dd::tui::app::{App, Level, Tab};
use neutron_transport_dd::tui::model::Validation;
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use tempfile::TempDir;

fn render(app: &mut App, width: u16, height: u16) -> String {
    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend).expect("terminal");
    terminal.draw(|frame| app.render(frame)).expect("draw");
    buffer_text(&terminal)
}

fn buffer_text(terminal: &Terminal<TestBackend>) -> String {
    let buffer = terminal.backend().buffer();
    let mut text = String::new();
    for y in 0..buffer.area.height {
        for x in 0..buffer.area.width {
            text.push_str(buffer[(x, y)].symbol());
        }
        text.push('\n');
    }
    text
}

fn press(app: &mut App, code: KeyCode) {
    app.handle_key(KeyEvent::new(code, KeyModifiers::NONE));
}

fn type_text(app: &mut App, text: &str) {
    for character in text.chars() {
        press(app, KeyCode::Char(character));
    }
}

#[test]
fn config_screen_renders() {
    let mut app = App::default();
    let text = render(&mut app, 110, 34);
    assert!(text.contains("nts-rs"));
    assert!(text.contains("Config"));
    assert!(text.contains("Configuration"));
    assert!(text.contains("Regions"));
    assert!(text.contains("Validation"));
    assert!(text.contains("Geometry"));
    assert!(text.contains("Valid"));
}

#[test]
fn small_terminal_shows_a_warning() {
    let mut app = App::default();
    let text = render(&mut app, 60, 20);
    assert!(text.contains("Terminal too small"));
    assert!(text.contains("80x24"));
}

#[test]
fn editing_region_count_adds_rows() {
    let mut app = App::default();
    assert_eq!(app.model.num_regions, 1);

    press(&mut app, KeyCode::Enter); // first row: Regions
    assert!(app.editing.is_some());
    press(&mut app, KeyCode::Backspace);
    type_text(&mut app, "2");
    press(&mut app, KeyCode::Enter);

    assert_eq!(app.model.num_regions, 2);
    let labels: Vec<String> = app
        .model
        .fields()
        .into_iter()
        .map(|field| field.label)
        .collect();
    assert!(labels.contains(&"NC region 2".to_string()));
    assert!(matches!(app.validation, Validation::Valid(_)));
}

#[test]
fn quadrature_cycles_with_arrows() {
    let mut app = App::default();
    let index = app
        .model
        .fields()
        .iter()
        .position(|field| field.label == "N")
        .expect("quadrature row");
    app.selected = index;
    assert_eq!(app.model.n, 8);
    press(&mut app, KeyCode::Right);
    assert_eq!(app.model.n, 2);
    press(&mut app, KeyCode::Left);
    assert_eq!(app.model.n, 8);
}

#[test]
fn saving_writes_a_loadable_configuration() {
    let home = TempDir::new().expect("temp");
    let mut app = App::default();
    app.model.nc[0] = "40".into();
    app.model.nu_sigf[0] = "0.11".into();
    app.model.mode = 3; // eigenvalue

    let path = home.path().join("config.json");
    app.save_to(&path).expect("save");

    let config = Config::from_json_file(&path, 1e-5, 2000, None).expect("valid config");
    assert_eq!(config.ntc, 40);
    assert_eq!(config.mode.as_str(), "eigenvalue");
}

#[test]
fn invalid_configuration_is_not_saved() {
    let mut app = App::default();
    app.model.sct[0] = "0".into();
    app.save();
    assert_eq!(app.level, Level::Error);
    assert!(app.status.starts_with("Cannot save"));
}

#[test]
fn placeholders_are_reported_as_a_template() {
    let mut app = App::default();
    app.model.scs[0] = "kk".into();
    app.revalidate();
    assert!(
        matches!(&app.validation, Validation::Template(names) if names == &vec!["kk".to_string()])
    );
    assert!(app.status.contains("placeholders"));
}

#[test]
fn tabs_switch_and_render_their_screens() {
    let mut app = App::default();
    press(&mut app, KeyCode::Tab);
    assert_eq!(app.tab, Tab::Run);
    let text = render(&mut app, 100, 30);
    assert!(text.contains("Configs ("));
    assert!(text.contains("Log"));

    press(&mut app, KeyCode::Tab);
    assert_eq!(app.tab, Tab::Results);
    let text = render(&mut app, 100, 30);
    assert!(text.contains("Outputs ("));

    press(&mut app, KeyCode::Tab);
    assert_eq!(app.tab, Tab::Plots);
    let text = render(&mut app, 100, 30);
    assert!(text.contains("next phase"));
}

#[test]
fn toggling_a_flag_does_not_open_the_editor() {
    let mut app = App::default();
    let index = app
        .model
        .fields()
        .iter()
        .position(|field| field.label == "Reflective left")
        .expect("row");
    app.selected = index;
    press(&mut app, KeyCode::Enter);
    assert!(app.editing.is_none());
    assert!(app.model.reflex_izq);
}
