//! TUI mouse, accordion and dropdown tests.

use neutron_transport_dd::tui::app::{Action, App, Selection, Tab};
use neutron_transport_dd::tui::model::FieldKind;
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::crossterm::event::{
    KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use ratatui::layout::Rect;

fn draw(app: &mut App) -> Terminal<TestBackend> {
    let backend = TestBackend::new(120, 40);
    let mut terminal = Terminal::new(backend).expect("terminal");
    terminal.draw(|frame| app.render(frame)).expect("draw");
    terminal
}

fn text(terminal: &Terminal<TestBackend>) -> Vec<String> {
    let buffer = terminal.backend().buffer();
    (0..buffer.area.height)
        .map(|y| {
            (0..buffer.area.width)
                .map(|x| buffer[(x, y)].symbol())
                .collect::<String>()
        })
        .collect()
}

fn find(terminal: &Terminal<TestBackend>, needle: &str) -> (u16, u16) {
    for (y, row) in text(terminal).iter().enumerate() {
        if let Some(byte_index) = row.find(needle) {
            return (byte_index as u16, y as u16);
        }
    }
    panic!("text not found: {needle}");
}

fn click(app: &mut App, column: u16, row: u16) {
    app.handle_mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column,
        row,
        modifiers: KeyModifiers::NONE,
    });
}

fn wheel(app: &mut App, up: bool, column: u16, row: u16) {
    app.handle_mouse(MouseEvent {
        kind: if up {
            MouseEventKind::ScrollUp
        } else {
            MouseEventKind::ScrollDown
        },
        column,
        row,
        modifiers: KeyModifiers::NONE,
    });
}

fn center(rect: Rect) -> (u16, u16) {
    (rect.x + rect.width / 2, rect.y + rect.height / 2)
}

#[test]
fn tabs_respond_to_clicks() {
    let mut app = App::default();
    let terminal = draw(&mut app);
    let _ = terminal;
    let rect = app
        .hotspots()
        .into_iter()
        .find(|(_, action)| *action == Action::Tab(Tab::Results))
        .map(|(rect, _)| rect)
        .expect("results tab");
    let (x, y) = center(rect);
    click(&mut app, x, y);
    assert_eq!(app.tab, Tab::Results);
}

#[test]
fn section_headers_collapse_and_expand() {
    let mut app = App::default();
    let terminal = draw(&mut app);
    let (x, y) = find(&terminal, "Geometry");
    click(&mut app, x, y);
    assert!(
        app.collapsed_sections
            .contains(&neutron_transport_dd::tui::app::SectionId::Geometry)
    );

    let terminal = draw(&mut app);
    assert!(
        !text(&terminal)
            .iter()
            .any(|row| row.contains("NC region 1"))
    );

    let (x, y) = find(&terminal, "Geometry");
    click(&mut app, x, y);
    assert!(
        !app.collapsed_sections
            .contains(&neutron_transport_dd::tui::app::SectionId::Geometry)
    );
}

#[test]
fn zone_headers_collapse_their_fields() {
    let mut app = App::default();
    app.model.num_zones = 2;
    app.model.sct.push("1.0".into());
    app.model.scs.push("0.9".into());
    app.model.nu_sigf.push("0.0".into());
    app.model.sigma_f.push(String::new());

    let terminal = draw(&mut app);
    let (x, y) = find(&terminal, "Zone 2");
    click(&mut app, x, y);
    assert!(app.collapsed_zones.contains(&1));

    let terminal = draw(&mut app);
    assert!(!text(&terminal).iter().any(|row| row.contains("SCT zone 2")));
    assert!(text(&terminal).iter().any(|row| row.contains("SCT zone 1")));
}

#[test]
fn double_click_on_a_field_opens_the_editor() {
    let mut app = App::default();
    let terminal = draw(&mut app);
    let (x, y) = find(&terminal, "Regions");
    click(&mut app, x, y);
    click(&mut app, x, y);
    assert!(app.editing.is_some());
}

#[test]
fn clicking_a_field_selects_it() {
    let mut app = App::default();
    let terminal = draw(&mut app);
    let (x, y) = find(&terminal, "SCS zone 1");
    click(&mut app, x, y);
    assert_eq!(app.selection, Selection::Field(FieldKind::ZoneSigmaS(0)));
}

#[test]
fn scheme_dropdown_opens_and_selects() {
    let mut app = App::default();
    let terminal = draw(&mut app);
    let (x, y) = find(&terminal, "Scheme");
    click(&mut app, x, y);
    click(&mut app, x, y);

    let terminal = draw(&mut app);
    assert!(
        text(&terminal)
            .iter()
            .any(|row| row.contains("Spatial scheme"))
    );
    let (x, y) = find(&terminal, "STEP");
    click(&mut app, x, y);

    assert_eq!(app.model.scheme, 1);
    assert!(!app.popup_open());
}

#[test]
fn keyboard_dropdown_for_mode() {
    let mut app = App::default();
    app.select_field(FieldKind::Mode);
    app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    draw(&mut app);
    app.handle_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
    app.handle_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
    app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    assert_eq!(app.model.mode, 2); // driven
}

#[test]
fn keyboard_collapses_all_zones_and_sections() {
    let mut app = App::default();
    app.handle_key(KeyEvent::new(KeyCode::Char('z'), KeyModifiers::NONE));
    assert_eq!(app.collapsed_zones.len(), app.model.num_zones);
    app.handle_key(KeyEvent::new(KeyCode::Char('z'), KeyModifiers::NONE));
    assert!(app.collapsed_zones.is_empty());

    app.handle_key(KeyEvent::new(KeyCode::Char('Z'), KeyModifiers::SHIFT));
    assert_eq!(
        app.collapsed_sections.len(),
        neutron_transport_dd::tui::app::SectionId::ALL.len()
    );
}

#[test]
fn wheel_scrolls_the_form_and_the_log() {
    let mut app = App::default();
    let terminal = draw(&mut app);
    let (x, y) = find(&terminal, "Regions");
    wheel(&mut app, false, x, y);
    assert_ne!(app.selection, Selection::Field(FieldKind::NumRegions));

    app.switch_tab(Tab::Run);
    app.run.log = (0..100).map(|index| format!("line {index}")).collect();
    let terminal = draw(&mut app);
    let (x, y) = find(&terminal, "Log");
    wheel(&mut app, true, x, y);
    assert!(app.run.log_offset > 0);
}

#[test]
fn mouse_capture_can_be_toggled_with_m() {
    let mut app = App::default();
    assert!(app.mouse_captured);
    app.handle_key(KeyEvent::new(KeyCode::Char('m'), KeyModifiers::NONE));
    assert!(!app.mouse_captured);
    app.handle_key(KeyEvent::new(KeyCode::Char('m'), KeyModifiers::NONE));
    assert!(app.mouse_captured);
}

#[test]
fn plot_kind_button_cycles() {
    let mut app = App::default();
    app.switch_tab(Tab::Plots);
    draw(&mut app);
    let rect = app
        .hotspots()
        .into_iter()
        .find(|(_, action)| *action == Action::NextKind)
        .map(|(rect, _)| rect)
        .expect("kind button");
    let (x, y) = center(rect);
    click(&mut app, x, y);
    assert_eq!(app.plots.kind, 1);
}
