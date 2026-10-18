//! tally: count lines, words and bytes, like a tiny `wc`.

mod args;
mod count;
mod report;

use std::process::ExitCode;

fn main() -> ExitCode {
    let opts = match args::parse(std::env::args().skip(1)) {
        Ok(opts) => opts,
        Err(msg) => {
            eprintln!("tally: {msg}");
            return ExitCode::from(2);
        }
    };

    let mut rows = Vec::new();
    for path in &opts.paths {
        match std::fs::read_to_string(path) {
            Ok(text) => rows.push((path.clone(), count::count(&text))),
            Err(e) => eprintln!("tally: {path}: {e}"),
        }
    }

    print!("{}", report::table(&rows, opts.sort));
    ExitCode::SUCCESS
}
