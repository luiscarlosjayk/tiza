//! The numbers that the docs give for constants of the code: each one is
//! the value of its constant.
//!
//! A few words around each number find it on the pages ([`Cite`]). Each
//! group of words must find at least as many numbers as the docs give
//! today. So a page that loses its cite fails, and a page that gives
//! another number fails too.

use std::time::Duration;

use tiza_qa::markdown::cited;
use tiza_qa::repo_file;
use tiza_tui::app::PRESENT_MESSAGE_TTL;
use tiza_tui::appearance::{COLOR_QUERY_LATE_FACTOR, COLOR_QUERY_WAIT};
use tiza_tui::ipc::ANSWER_TIMEOUT;

use crate::common::repository_files;

/// The words before and after a number, and how many times the docs have
/// them today.
struct Cite {
    before: &'static str,
    after: &'static str,
    min: usize,
}

/// A value of the code: its name, its number, and the cites that give it.
struct Value<'a> {
    name: &'a str,
    number: u64,
    cites: &'a [Cite],
}

/// The pages that a user reads: the README and the docs, but the release
/// notes, which give the values of their date.
fn pages() -> Vec<(String, String)> {
    repository_files()
        .into_iter()
        .filter(|f| {
            f == "README.md"
                || (f.starts_with("docs/")
                    && f.ends_with(".md")
                    && !f.starts_with("docs/release-notes/"))
        })
        .map(|f| {
            let text = repo_file(&f);
            (f, text)
        })
        .collect()
}

/// Each cite of each value must give its number, and each group of words
/// must come at least its minimum of times.
fn check(unit: &str, values: &[Value]) {
    let pages = pages();
    let mut wrong = Vec::new();
    for v in values {
        for c in v.cites {
            let mut found = 0;
            for (page, text) in &pages {
                for (line, n) in cited(text, c.before, c.after) {
                    found += 1;
                    if n != v.number {
                        wrong.push(format!(
                            "{page}:{line} says {n}, and {} is {} {unit}. If this number is \
                             another constant, make the words around it in this test longer",
                            v.name, v.number
                        ));
                    }
                }
            }
            if found < c.min {
                wrong.push(format!(
                    "expected at least {} cites of «{}N{}», found {found}: a page lost its \
                     cite (put the words back), or change the minimum in this test",
                    c.min, c.before, c.after
                ));
            }
        }
    }
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}

/// `d` in seconds. The docs give whole seconds.
fn whole_seconds(name: &str, d: Duration) -> u64 {
    assert_eq!(
        d.subsec_nanos(),
        0,
        "{name} is {d:?}, and the docs give it in whole seconds"
    );
    d.as_secs()
}

#[test]
fn the_docs_give_the_wait_of_the_socket_client() {
    check(
        "s",
        &[Value {
            name: "ANSWER_TIMEOUT",
            number: whole_seconds("ANSWER_TIMEOUT", ANSWER_TIMEOUT),
            cites: &[
                Cite {
                    before: "answer within ",
                    after: " s",
                    min: 4,
                },
                Cite {
                    before: "waits ",
                    after: " s for a response",
                    min: 1,
                },
            ],
        }],
    );
}

#[test]
fn the_docs_give_how_long_a_command_result_shows_in_presentation_mode() {
    check(
        "s",
        &[Value {
            name: "PRESENT_MESSAGE_TTL",
            number: whole_seconds("PRESENT_MESSAGE_TTL", PRESENT_MESSAGE_TTL),
            cites: &[
                Cite {
                    before: "stays there for ",
                    after: " seconds",
                    min: 1,
                },
                Cite {
                    before: "last row for ",
                    after: " seconds",
                    min: 2,
                },
            ],
        }],
    );
}

#[test]
fn the_docs_give_the_waits_of_the_color_query() {
    let wait = COLOR_QUERY_WAIT.as_millis() as u64;
    let late = wait * u64::from(COLOR_QUERY_LATE_FACTOR);
    check(
        "ms",
        &[
            Value {
                name: "COLOR_QUERY_WAIT",
                number: wait,
                cites: &[
                    Cite {
                        before: "nothing comes for ",
                        after: " ms",
                        min: 1,
                    },
                    Cite {
                        before: "wait is ",
                        after: " ms when",
                        min: 1,
                    },
                    Cite {
                        before: "after ",
                        after: " ms with no new data",
                        min: 1,
                    },
                    Cite {
                        before: "no answer for ",
                        after: " ms",
                        min: 1,
                    },
                ],
            },
            Value {
                name: "COLOR_QUERY_WAIT × COLOR_QUERY_LATE_FACTOR",
                number: late,
                cites: &[Cite {
                    before: "more than ",
                    after: " ms after the color query",
                    min: 1,
                }],
            },
            Value {
                name: "that time plus COLOR_QUERY_WAIT",
                number: late + wait,
                cites: &[Cite {
                    before: "can be almost ",
                    after: " ms",
                    min: 1,
                }],
            },
        ],
    );
}
