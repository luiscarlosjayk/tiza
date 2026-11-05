//! Each `:` command, socket method, MCP tool and CLI flag has its place in
//! the reference: a heading or a row of a table that names it. Each list
//! comes from the code: `App::COMMANDS`, `protocol::METHODS`, the
//! `tools/list` of `tiza mcp`, and the `--help` of `tiza` and of each
//! subcommand (a hidden flag is not in the help, so it needs no row). A new
//! item with no row stops the check, with the page where its row goes.

use tiza_qa::{McpClient, command, repo_file, repo_root, scratch};

/// The first cell of each row of a table of `page`.
fn first_cells(page: &str) -> Vec<&str> {
    page.lines()
        .filter_map(|l| l.strip_prefix('|'))
        .filter_map(|l| l.split('|').next())
        .collect()
}

/// Whether `text` has a code span that starts with `name`, alone or with
/// its arguments: `` `--focus` `` or `` `--focus A-B` ``, not `` `--focuses` ``.
fn names(text: &str, name: &str) -> bool {
    let start = format!("`{name}");
    text.match_indices(&start)
        .any(|(i, _)| matches!(text[i + start.len()..].chars().next(), Some(' ' | '`')))
}

/// Whether a row of `page` names `name` in its first cell.
fn has_row(page: &str, name: &str) -> bool {
    first_cells(page).iter().any(|c| names(c, name))
}

/// Whether `page` has a heading or a row for the `:` command `name`.
fn has_command(page: &str, name: &str) -> bool {
    let name = format!(":{name}");
    page.lines().any(|l| l.starts_with('#') && names(l, &name)) || has_row(page, &name)
}

/// The pages of `docs/reference/<dir>`, with their text, in order.
fn pages(dir: &str) -> Vec<(String, String)> {
    let root = repo_root().join("docs/reference").join(dir);
    let mut out: Vec<(String, String)> = std::fs::read_dir(&root)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|x| x == "md"))
        .map(|p| {
            let rel = p.strip_prefix(repo_root()).unwrap().to_string_lossy();
            (rel.replace('\\', "/"), std::fs::read_to_string(&p).unwrap())
        })
        .collect();
    out.sort();
    out
}

/// Fails with each item of `missing` and where its row goes.
fn report(missing: &[String], kind: &str, place: &str) {
    assert!(
        missing.is_empty(),
        "{} {kind} with no heading or row in {place}: {}. Add each one there.",
        missing.len(),
        missing.join(", ")
    );
}

/// `tiza [SUB…] --help`: the long flags of its options, and its
/// subcommands (not `help`).
fn help_of(path: &[String]) -> (Vec<String>, Vec<String>) {
    let out = command()
        .args(path)
        .arg("--help")
        .output()
        .expect("run tiza --help");
    assert!(
        out.status.success(),
        "tiza {} --help failed",
        path.join(" ")
    );
    let text = String::from_utf8(out.stdout).unwrap();
    let mut flags = Vec::new();
    let mut subs = Vec::new();
    let mut in_commands = false;
    for line in text.lines() {
        if !line.starts_with(' ') {
            in_commands = line == "Commands:";
            continue;
        }
        let words: Vec<&str> = line.split_whitespace().collect();
        if in_commands {
            if let Some(&name) = words.first()
                && name != "help"
            {
                subs.push(name.to_string());
            }
            continue;
        }
        // `  -h, --help`, `      --width <N>`: the first words of an option.
        for w in words.iter().take(2) {
            let w = w.trim_end_matches(',');
            if let Some(name) = w.strip_prefix("--")
                && name.chars().next().is_some_and(|c| c.is_ascii_lowercase())
            {
                flags.push(format!("--{}", name.split(['=', '<']).next().unwrap()));
            }
        }
    }
    (flags, subs)
}

#[test]
fn each_colon_command_has_a_heading_or_a_row_in_the_command_pages() {
    let pages = pages("commands");
    let missing: Vec<String> = tiza_tui::app::App::COMMANDS
        .iter()
        .filter(|c| !pages.iter().any(|(_, text)| has_command(text, c)))
        .map(|c| format!("`:{c}`"))
        .collect();
    report(
        &missing,
        "`:` commands",
        "docs/reference/commands/ (the page of its task: see commands.md)",
    );
}

#[test]
fn each_socket_method_has_a_row_in_the_socket_api_page() {
    let page = repo_file("docs/reference/socket-api.md");
    let missing: Vec<String> = tiza_core::protocol::METHODS
        .iter()
        .filter(|(m, _)| !has_row(&page, m))
        .map(|(m, _)| format!("`{m}`"))
        .collect();
    report(&missing, "socket methods", "docs/reference/socket-api.md");
}

#[test]
fn each_mcp_tool_has_a_row_in_the_mcp_tools_page() {
    let page = repo_file("docs/reference/mcp-tools.md");
    // `tools/list` needs no live instance: the socket is never opened.
    let socket = scratch("reference-rows").join("none.sock");
    let mut mcp = McpClient::start(&socket);
    let tools = mcp.tool_names();
    assert!(!tools.is_empty(), "tools/list gave no tool");
    let missing: Vec<String> = tools
        .iter()
        .filter(|t| !has_row(&page, t))
        .map(|t| format!("`{t}`"))
        .collect();
    report(&missing, "MCP tools", "docs/reference/mcp-tools.md");
}

#[test]
fn each_cli_flag_has_a_row_in_the_reference() {
    let pages = pages("");
    let mut missing = Vec::new();
    let mut todo = vec![Vec::<String>::new()];
    let mut seen = 0;
    while let Some(path) = todo.pop() {
        let (flags, subs) = help_of(&path);
        for f in flags {
            seen += 1;
            if !pages.iter().any(|(_, text)| has_row(text, &f)) {
                missing.push(format!(
                    "`tiza {}{f}`",
                    path.iter().map(|s| format!("{s} ")).collect::<String>()
                ));
            }
        }
        todo.extend(subs.into_iter().map(|s| [path.clone(), vec![s]].concat()));
    }
    assert!(
        seen > 20,
        "only {seen} flags in the help: the parse of --help is wrong"
    );
    missing.sort();
    missing.dedup();
    report(
        &missing,
        "CLI flags",
        "a table of docs/reference/ (cli.md, or the page of its command)",
    );
}

/// The matchers see a page without the row, so the checks above can fail.
#[test]
fn a_page_without_the_row_is_seen() {
    let page =
        "| Method | What |\n|---|---|\n| `status` | The state. |\n\nText with `focus.set`.\n";
    assert!(has_row(page, "status"));
    assert!(
        !has_row(page, "focus.set"),
        "a name in the text is not a row"
    );
    let cmds = "## `:step N`\n\n| Command | What |\n|---|---|\n| `:undo` | Undoes. |\n\nSee `:wq` and `:stepper`.\n";
    assert!(has_command(cmds, "step"));
    assert!(has_command(cmds, "undo"));
    assert!(
        !has_command(cmds, "wq"),
        "a command in the text is not a row"
    );
    assert!(!has_command(cmds, "ste"), "`:step` is not `:ste`");
    let flags = "| `-V` or `--version` | Prints the version. |\n| `--focus A-B` | Dims. |\n";
    assert!(has_row(flags, "--version"));
    assert!(has_row(flags, "--focus"), "a flag with its argument");
    assert!(!has_row(flags, "--verbose"));
    assert!(!has_row(flags, "--foc"), "`--focus` is not `--foc`");
}
