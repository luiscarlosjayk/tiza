//! `:set ID text=…` changes the text that the user sees as the text: the
//! text of a note, the paragraph of a callout. `label=` and `title=` change
//! the title of a callout. Before, `text=` on a callout changed its title.

use tiza_core::model::Shape;
use tiza_qa::Live;

const CODE: &str = "fn main() {\n    let a = 1;\n    let b = 2;\n}\n";

fn callout(live: &Live, id: &str) -> (Option<String>, String) {
    let id = id.to_string();
    live.with(
        move |app| match &app.doc.find_annotation(&id).unwrap().shape {
            Shape::Callout { title, text, .. } => (title.clone(), text.clone()),
            other => panic!("{other:?}"),
        },
    )
}

#[test]
fn text_is_the_paragraph_of_a_callout_and_label_its_title() {
    let live = Live::from_code(CODE, "rs", "set-callout");
    live.command("step 1");
    live.command("callout 2-3 @below title=\"Old title\" old paragraph");
    live.command("set a1 text=\"new paragraph\"");
    assert_eq!(
        callout(&live, "a1"),
        (Some("Old title".into()), "new paragraph".into()),
        "{:?}",
        live.message()
    );
    live.command("set a1 label=\"New title\"");
    assert_eq!(callout(&live, "a1").0.as_deref(), Some("New title"));
    live.command("set a1 title=Third");
    assert_eq!(callout(&live, "a1").0.as_deref(), Some("Third"));
    // The paragraph and a frame key in one command: one undo step, which
    // takes back both.
    let border = |live: &Live| {
        live.with(|app| match &app.doc.find_annotation("a1").unwrap().shape {
            Shape::Callout { frame, .. } => frame.border,
            other => panic!("{other:?}"),
        })
    };
    let before = border(&live);
    live.command("set a1 text=framed border=double");
    assert_ne!(border(&live), before);
    live.command("undo");
    assert_eq!(callout(&live, "a1").1, "new paragraph");
    assert_eq!(border(&live), before, "one undo took back the border too");
    // An empty value is refused: a callout keeps a paragraph.
    live.command("set a1 text=\"\"");
    let (message, error) = live.message().unwrap_or_default();
    assert!(error, "{message}");
    assert_eq!(callout(&live, "a1").1, "new paragraph");
    // On a note, `text=` is still its text.
    live.command("note 2 a note");
    live.command("set a2 text=changed");
    let note = live.with(|app| match &app.doc.find_annotation("a2").unwrap().shape {
        Shape::Note { text, .. } => text.clone(),
        other => panic!("{other:?}"),
    });
    assert_eq!(note, "changed");
}
