//! TUI phase A: config screen rendering, editing, validation and saving.

use neutron_transport_dd::config::{Config, RunOverrides};
use neutron_transport_dd::tui::app::{App, Level, Tab};
use neutron_transport_dd::tui::model::{FieldKind, Validation};
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
    app.select_field(FieldKind::Quadrature);
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
    app.model.epsilon = "1e-6".into();
    app.model.max_iter = "500".into();
    app.model.zone_names = vec!["Core".into()];

    let path = home.path().join("config.json");
    app.save_to(&path).expect("save");

    let config =
        Config::from_json_file_with(&path, RunOverrides::default(), None).expect("valid config");
    assert_eq!(config.ntc, 40);
    assert_eq!(config.mode.as_str(), "eigenvalue");
    assert_eq!(config.epsilon, 1e-6);
    assert_eq!(config.max_iter, 500);
    assert_eq!(config.zone_names, Some(vec!["Core".to_string()]));
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
    assert!(text.contains("kind:"));

    press(&mut app, KeyCode::Tab);
    assert_eq!(app.tab, Tab::Placeholders);
    let text = render(&mut app, 100, 30);
    assert!(text.contains("Placeholders"));

    press(&mut app, KeyCode::Tab);
    assert_eq!(app.tab, Tab::Export);
    let text = render(&mut app, 100, 30);
    assert!(text.contains("datos.json"));
}

#[test]
fn toggling_a_flag_does_not_open_the_editor() {
    let mut app = App::default();
    app.select_field(FieldKind::ReflexLeft);
    press(&mut app, KeyCode::Enter);
    assert!(app.editing.is_none());
    assert!(app.model.reflex_izq);
}

#[test]
fn adding_zones_pads_zone_names() {
    let mut app = App::default();
    app.model.zone_names = vec!["Reflector".into()];
    app.model
        .set_field(neutron_transport_dd::tui::model::FieldKind::NumRegions, "3");
    app.model
        .set_field(neutron_transport_dd::tui::model::FieldKind::NumZones, "3");
    assert_eq!(app.model.zone_names, vec!["Reflector", "Zone 2", "Zone 3"]);
    let validation = app.model.validate();
    assert!(matches!(validation, Validation::Valid(_)), "{validation:?}");
}

#[test]
fn editor_overlay_stays_narrow() {
    let mut app = App::default();
    press(&mut app, KeyCode::Enter); // edit Regions
    assert!(app.editing.is_some());

    let text = render(&mut app, 120, 40);
    let line = text
        .lines()
        .find(|line| line.contains('┌') && line.contains("Regions"))
        .expect("editor line");
    let start = line
        .chars()
        .position(|character| character == '┌')
        .expect("left border");
    let end = line
        .chars()
        .position(|character| character == '┐')
        .expect("right border");
    assert_eq!(end - start + 1, 22);
}

#[test]
fn long_editor_values_show_their_tail() {
    let mut app = App::default();
    press(&mut app, KeyCode::Enter);
    press(&mut app, KeyCode::Backspace);
    type_text(
        &mut app,
        "123456789012345678901234567890123456789012345678901234567890",
    );

    let text = render(&mut app, 120, 40);
    assert!(text.contains("…"), "long value should keep the end visible");
}

#[test]
fn s_opens_the_save_as_dialog_with_the_default_path() {
    let mut app = App::default();
    press(&mut app, KeyCode::Char('s'));
    assert!(app.save_dialog_open());
    let value = app.save_dialog_value().expect("path");
    assert!(
        value
            .replace('\\', "/")
            .contains("outputs/inputs/config_1d_")
    );
    assert!(value.ends_with(".json"));

    let text = render(&mut app, 120, 40);
    assert!(text.contains("Save configuration"));
    assert!(text.contains("config_1d_"));

    press(&mut app, KeyCode::Esc);
    assert!(!app.save_dialog_open());
}

#[test]
fn save_dialog_writes_the_edited_path() {
    let home = TempDir::new().expect("temp");
    let mut app = App::default();
    let target = home.path().join("custom");
    app.open_save_dialog_with(target);
    type_text(&mut app, ".json");
    press(&mut app, KeyCode::Enter);

    let path = home.path().join("custom.json");
    assert!(path.exists());
    Config::from_json_file_with(&path, RunOverrides::default(), None).expect("valid");
    assert!(!app.save_dialog_open());
    assert!(app.status.contains("Saved"));
}

#[test]
fn save_dialog_rejects_non_json_paths() {
    let home = TempDir::new().expect("temp");
    let mut app = App::default();
    app.open_save_dialog_with(home.path().join("config.txt"));
    press(&mut app, KeyCode::Enter);

    assert!(app.save_dialog_open());
    assert!(app.status.contains("must end with .json"));
    assert!(!home.path().join("config.txt").exists());
}

#[test]
fn save_dialog_is_wider_than_the_field_editor() {
    let mut app = App::default();
    app.open_save_dialog_with(std::path::PathBuf::from("/tmp/example.json"));
    let text = render(&mut app, 120, 40);
    let line = text
        .lines()
        .find(|line| line.contains("Save configuration"))
        .expect("dialog line");
    let start = line
        .chars()
        .position(|character| character == '┌')
        .expect("left");
    let end = line
        .chars()
        .position(|character| character == '┐')
        .expect("right");
    assert_eq!(end - start + 1, 76);
}

#[test]
fn cursor_moves_and_inserts_in_the_middle() {
    let mut app = App::default();
    app.select_field(FieldKind::RegionCells(0));
    press(&mut app, KeyCode::Enter);
    assert!(app.editing.is_some());

    press(&mut app, KeyCode::Home);
    type_text(&mut app, "1");
    press(&mut app, KeyCode::Enter);
    assert_eq!(app.model.nc[0], "120");
}

#[test]
fn delete_and_backspace_follow_the_cursor() {
    let mut app = App::default();
    app.select_field(FieldKind::RegionCells(0));
    press(&mut app, KeyCode::Enter);
    press(&mut app, KeyCode::Left);
    press(&mut app, KeyCode::Backspace);
    press(&mut app, KeyCode::Enter);
    assert_eq!(app.model.nc[0], "0");

    let mut app = App::default();
    app.select_field(FieldKind::RegionCells(0));
    press(&mut app, KeyCode::Enter);
    press(&mut app, KeyCode::Home);
    press(&mut app, KeyCode::Delete);
    press(&mut app, KeyCode::Enter);
    assert_eq!(app.model.nc[0], "0");
}

#[test]
fn geometry_panel_shows_only_one_length_reference() {
    let mut app = App::default();
    let text = render(&mut app, 120, 40);
    assert!(text.contains("Geometry"));
    assert!(text.contains("█"));
    assert!(text.contains("20.00 cm"));
    assert!(!text.contains("total:"));
}

#[test]
fn editor_cursor_does_not_split_the_number() {
    let mut app = App::default();
    app.select_field(FieldKind::RegionThickness(0));
    press(&mut app, KeyCode::Enter);
    press(&mut app, KeyCode::Home);
    press(&mut app, KeyCode::Right);

    let text = render(&mut app, 120, 40);
    let lines: Vec<&str> = text.lines().collect();
    let border = lines
        .iter()
        .position(|line| line.contains('┌') && line.contains("HR region 1"))
        .expect("editor border");
    let content = lines[border + 1];
    assert!(content.contains("20"));
    assert!(!content.contains('▏'));
}

#[test]
fn config_screen_shows_saved_panel_and_placeholder_hint() {
    let home = TempDir::new().expect("temp");
    std::fs::write(home.path().join("config_1d_001.json"), "{}").unwrap();

    let mut app = App::default();
    app.saved = neutron_transport_dd::tui::services::config_store::list_from(home.path());
    let text = render(&mut app, 140, 40);
    assert!(text.contains("Configs ("));
    assert!(text.contains("config_1d_001.json"));
    assert!(text.contains("kk/mm = placeholders"));
}
