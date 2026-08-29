use std::{
    io::{BufRead as _, BufReader, Write as _},
    process::{Command, Stdio},
};

use serde_json::{Value, json};

#[test]
fn stdio_server_supports_discovery_tools_and_transactions() {
    let mut child = Command::new(env!("CARGO_BIN_EXE_gpui-design-mcp"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn gpui-design-mcp");
    let mut stdin = child.stdin.take().expect("child stdin");
    let stdout = child.stdout.take().expect("child stdout");
    let mut reader = BufReader::new(stdout);

    let discover = exchange(
        &mut stdin,
        &mut reader,
        json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "server/discover",
            "params": {
                "_meta": {
                    "io.modelcontextprotocol/protocolVersion": "2026-07-28",
                    "io.modelcontextprotocol/clientInfo": {
                        "name": "gpui-design-test",
                        "version": "1"
                    },
                    "io.modelcontextprotocol/clientCapabilities": {}
                }
            }
        }),
    );
    assert_eq!(discover["result"]["resultType"], "complete");
    assert!(
        discover["result"]["supportedVersions"]
            .as_array()
            .expect("supported versions")
            .iter()
            .any(|version| version == "2026-07-28")
    );

    let create = exchange(
        &mut stdin,
        &mut reader,
        tool_call(
            2,
            "gpui_design_create_document",
            json!({
                "document_id": "orbitline/login",
                "name": "Login"
            }),
        ),
    );
    assert_eq!(create["result"]["isError"], false);

    let apply = exchange(
        &mut stdin,
        &mut reader,
        tool_call(
            3,
            "gpui_design_apply_transaction",
            json!({
                "transaction": {
                    "protocol_version": 1,
                    "transaction_id": "add-login-root",
                    "document_id": "orbitline/login",
                    "expected_revision": 0,
                    "commands": [{
                        "command": "CREATE_NODE",
                        "node": {
                            "id": "login/root",
                            "name": "Login Root",
                            "kind": "CONTAINER",
                            "children": [],
                            "visible": true,
                            "locked": false,
                            "layout": {
                                "flow": { "kind": "NONE" },
                                "horizontal": { "kind": "HUG" },
                                "vertical": { "kind": "HUG" },
                                "padding": {
                                    "top": { "literal": 0.0 },
                                    "right": { "literal": 0.0 },
                                    "bottom": { "literal": 0.0 },
                                    "left": { "literal": 0.0 }
                                },
                                "alignment": "START",
                                "cross_alignment": "START",
                                "distribution": "START",
                                "position": { "kind": "FLOW" },
                                "scroll": {
                                    "horizontal": false,
                                    "vertical": false
                                },
                                "clip": false
                            },
                            "visual": {
                                "fills": [],
                                "radii": {
                                    "top_left": { "literal": 0.0 },
                                    "top_right": { "literal": 0.0 },
                                    "bottom_right": { "literal": 0.0 },
                                    "bottom_left": { "literal": 0.0 }
                                },
                                "effects": [],
                                "opacity": 1.0
                            },
                            "token_bindings": {},
                            "tags": [],
                            "metadata": {}
                        },
                        "parent": null,
                        "index": null
                    }]
                }
            }),
        ),
    );
    assert_eq!(apply["result"]["isError"], false);
    assert_eq!(
        apply["result"]["structuredContent"]["receipt"]["revision"],
        1
    );

    let replay = exchange(
        &mut stdin,
        &mut reader,
        tool_call(
            4,
            "gpui_design_apply_transaction",
            json!({
                "transaction": {
                    "protocol_version": 1,
                    "transaction_id": "add-login-root",
                    "document_id": "orbitline/login",
                    "expected_revision": 0,
                    "commands": [{
                        "command": "CREATE_NODE",
                        "node": {
                            "id": "login/root",
                            "name": "Login Root",
                            "kind": "CONTAINER",
                            "children": [],
                            "visible": true,
                            "locked": false,
                            "layout": {
                                "flow": { "kind": "NONE" },
                                "horizontal": { "kind": "HUG" },
                                "vertical": { "kind": "HUG" },
                                "padding": {
                                    "top": { "literal": 0.0 },
                                    "right": { "literal": 0.0 },
                                    "bottom": { "literal": 0.0 },
                                    "left": { "literal": 0.0 }
                                },
                                "alignment": "START",
                                "cross_alignment": "START",
                                "distribution": "START",
                                "position": { "kind": "FLOW" },
                                "scroll": {
                                    "horizontal": false,
                                    "vertical": false
                                },
                                "clip": false
                            },
                            "visual": {
                                "fills": [],
                                "radii": {
                                    "top_left": { "literal": 0.0 },
                                    "top_right": { "literal": 0.0 },
                                    "bottom_right": { "literal": 0.0 },
                                    "bottom_left": { "literal": 0.0 }
                                },
                                "effects": [],
                                "opacity": 1.0
                            },
                            "token_bindings": {},
                            "tags": [],
                            "metadata": {}
                        },
                        "parent": null,
                        "index": null
                    }]
                }
            }),
        ),
    );
    assert_eq!(replay["result"]["structuredContent"]["replayed"], true);

    drop(stdin);
    assert!(child.wait().expect("wait for server").success());
}

fn tool_call(id: u64, name: &str, arguments: Value) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "method": "tools/call",
        "params": {
            "name": name,
            "arguments": arguments,
            "_meta": {
                "io.modelcontextprotocol/protocolVersion": "2026-07-28",
                "io.modelcontextprotocol/clientCapabilities": {}
            }
        }
    })
}

fn exchange(
    stdin: &mut std::process::ChildStdin,
    reader: &mut BufReader<std::process::ChildStdout>,
    request: Value,
) -> Value {
    serde_json::to_writer(&mut *stdin, &request).expect("serialize request");
    stdin.write_all(b"\n").expect("request newline");
    stdin.flush().expect("flush request");

    let mut response = String::new();
    reader.read_line(&mut response).expect("read response");
    assert!(!response.is_empty(), "server closed without response");
    serde_json::from_str(&response).expect("parse response")
}
