use codedraw_core::Document;
use codedraw_tui::app::{App, Mode};
use ratatui::{Terminal, backend::TestBackend};

#[test]
fn draw_after_linewise_box() {
    let doc = Document::from_code(
        "fn main() {\n    let s = String::from(\"hello\");\n    takes(s);\n    // c\n\n    let t = 1;\n}\n",
        "rs",
        None,
    );
    let mut app = App::new(doc, "/dev/null".into());
    app.autosave = false;
    let mut t = Terminal::new(TestBackend::new(100, 30)).unwrap();
    t.draw(|f| codedraw_tui::ui::draw(f, &mut app)).unwrap();
    app.cursor_line = 2;
    app.line_start();
    app.word_forward();
    app.select_word();
    app.select_action('h');
    t.draw(|f| codedraw_tui::ui::draw(f, &mut app)).unwrap();
    app.cursor_line = 3;
    app.line_start();
    app.select_word();
    app.select_action('m');
    assert_eq!(app.mode, Mode::Command("mark ".into()));
    app.mode = Mode::Normal;
    app.run_command("mark warn n");
    t.draw(|f| codedraw_tui::ui::draw(f, &mut app)).unwrap();
    app.start_select(true);
    app.move_cursor(1);
    app.move_cursor(1);
    t.draw(|f| codedraw_tui::ui::draw(f, &mut app)).unwrap();
    assert!(app.select_action('b'));
    assert_eq!(app.mode, Mode::Command("box 3-5 ".into()));
    app.mode = Mode::Normal;
    app.run_command("box 3-5 x");
    assert!(matches!(app.message, Some((_, false))), "{:?}", app.message);
    t.draw(|f| codedraw_tui::ui::draw(f, &mut app)).unwrap();
}
