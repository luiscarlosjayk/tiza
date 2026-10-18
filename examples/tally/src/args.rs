//! Command-line parsing by hand: no dependencies, one flag.

pub struct Options {
    pub paths: Vec<String>,
    pub sort: Sort,
}

#[derive(Clone, Copy)]
pub enum Sort {
    Name,
    Words,
}

pub fn parse(args: impl Iterator<Item = String>) -> Result<Options, String> {
    let mut paths = Vec::new();
    let mut sort = Sort::Name;
    for arg in args {
        if arg == "--by-words" {
            sort = Sort::Words;
        } else if arg.starts_with("--") {
            return Err(format!("unknown flag {arg}"));
        } else {
            paths.push(arg);
        }
    }
    if paths.is_empty() {
        return Err("usage: tally [--by-words] FILE...".into());
    }
    Ok(Options { paths, sort })
}
