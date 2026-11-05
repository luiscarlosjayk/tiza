//! The size of `tools/list`: what each agent that connects reads into its
//! context, before it calls a tool. Each change added detail to the
//! descriptions and schemas, and nothing watched the total. This test fixes
//! a budget at today's size with a margin. A change that makes the list
//! bigger than the budget must make the text shorter, or raise the budget
//! on purpose, with its reason, in `qa/budgets.json`.
//!
//! The size is the bytes of the tools of `tools/list` as compact JSON:
//! names, descriptions and input schemas. They come from the code, not from
//! an instance, so a server with no instance behind it gives the same list.

use serde_json::Value;
use tiza_qa::{McpClient, repo_root, scratch};

/// The `mcp_tools_list` entry of `qa/budgets.json`.
fn budget() -> Value {
    let p = repo_root().join("qa/budgets.json");
    let b: Value = serde_json::from_str(&std::fs::read_to_string(&p).unwrap())
        .unwrap_or_else(|e| panic!("{}: {e}", p.display()));
    b["mcp_tools_list"].clone()
}

#[test]
fn tools_list_stays_within_its_budget() {
    let b = budget();
    let limit = b["bytes"]
        .as_u64()
        .expect("mcp_tools_list.bytes in qa/budgets.json");
    let mut mcp = McpClient::start(&scratch("tools-list-budget").join("none.sock"));
    let tools = mcp.tools();
    // A broken reply (an error, or the list out of `result.tools`) is a few
    // bytes: it must not pass as a list within its budget.
    let n = tools.as_array().map_or(0, Vec::len);
    assert!(n > 0, "tools/list gave no tools: {tools}");
    let size = serde_json::to_vec(&tools).unwrap().len() as u64;
    if size <= limit {
        return;
    }
    let mut each: Vec<(u64, String)> = tools
        .as_array()
        .unwrap()
        .iter()
        .map(|t| {
            let n = serde_json::to_vec(t).unwrap().len() as u64;
            (n, t["name"].as_str().unwrap_or("?").to_string())
        })
        .collect();
    each.sort_by(|a, b| b.cmp(a));
    let biggest: Vec<String> = each
        .iter()
        .take(5)
        .map(|(n, name)| format!("{name} {n}"))
        .collect();
    panic!(
        "tools/list is {size} bytes, over its budget of {limit} bytes \
         (qa/budgets.json, mcp_tools_list). Each agent reads this list into its context. \
         Make a description or a schema shorter, or raise `bytes` and write a new `reason` \
         in the same commit. The biggest tools, in bytes: {}",
        biggest.join(", ")
    );
}

#[test]
fn the_budget_has_a_measure_and_a_reason() {
    let b = budget();
    let limit = b["bytes"].as_u64().expect("bytes");
    let measured = b["measured"].as_u64().expect("measured");
    assert!(measured <= limit, "{b}");
    let reason = b["reason"].as_str().unwrap_or("");
    assert!(!reason.trim().is_empty() && !reason.contains('\n'), "{b}");
}
