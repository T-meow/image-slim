use serde_json::{Value, json};
use std::io::{BufRead, BufReader, Write};
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::Duration;
use uuid::Uuid;

const AGENT_EXE: &str = env!("CARGO_BIN_EXE_image-slim-agent");

#[test]
fn generated_output_schema_is_bounded() {
    let schema = rmcp::handler::server::tool::schema_for_output::<
        image_slim_agent::protocol::Envelope<image_slim_agent::protocol::AgentCapabilities>,
    >()
    .unwrap();
    assert!(serde_json::to_vec(&schema).unwrap().len() < 8 * 1024);
}

#[test]
fn cli_stdout_is_one_json_result() {
    let output = Command::new(AGENT_EXE)
        .args(["capabilities", "--json"])
        .output()
        .unwrap();
    assert!(output.status.success());
    assert!(output.stderr.is_empty());
    let text = String::from_utf8(output.stdout).unwrap();
    assert_eq!(text.lines().count(), 1);
    let value: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(value["ok"], true);
    assert_eq!(value["result"]["agent_protocol_version"], 1);
}

#[test]
fn mcp_lists_five_bounded_tools_and_returns_structured_errors() {
    let mut child = Command::new(AGENT_EXE)
        .arg("mcp")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut stdin = child.stdin.take().unwrap();
    let stdout = child.stdout.take().unwrap();
    let (sender, receiver) = mpsc::channel();
    let reader = std::thread::spawn(move || {
        for line in BufReader::new(stdout).lines() {
            if sender.send(line).is_err() {
                break;
            }
        }
    });

    send(
        &mut stdin,
        json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {
                "protocolVersion": "2025-06-18",
                "capabilities": {},
                "clientInfo": {"name": "image-slim-test", "version": "1.0"}
            }
        }),
    );
    let initialized = receive(&receiver, "initialize");
    assert_eq!(initialized["id"], 1);
    assert!(initialized.get("result").is_some());
    send(
        &mut stdin,
        json!({"jsonrpc":"2.0","method":"notifications/initialized"}),
    );
    send(
        &mut stdin,
        json!({"jsonrpc":"2.0","id":2,"method":"tools/list"}),
    );
    let listed = receive(&receiver, "tools/list");
    let tools = listed["result"]["tools"].as_array().unwrap();
    assert_eq!(tools.len(), 5);
    assert!(tools.iter().all(|tool| tool.get("inputSchema").is_some()));
    assert!(tools.iter().all(|tool| tool.get("outputSchema").is_some()));
    let tools_size = serde_json::to_vec(tools).unwrap().len();
    assert!(tools_size <= 16 * 1024, "tools/list is {tools_size} bytes");

    send(
        &mut stdin,
        json!({
            "jsonrpc":"2.0",
            "id":3,
            "method":"tools/call",
            "params":{"name":"image_slim_capabilities","arguments":{}}
        }),
    );
    let capabilities = receive(&receiver, "capabilities");
    assert_eq!(
        capabilities["result"]["structuredContent"]["result"]["agent_protocol_version"],
        1
    );
    assert_eq!(
        capabilities["result"]["content"].as_array().unwrap().len(),
        1
    );

    send(
        &mut stdin,
        json!({
            "jsonrpc":"2.0",
            "id":4,
            "method":"tools/call",
            "params":{
                "name":"image_slim_plan",
                "arguments":{
                    "request_id":Uuid::new_v4().to_string(),
                    "paths":["D:\\not-allowed"]
                }
            }
        }),
    );
    let denied = receive(&receiver, "plan error");
    assert_eq!(denied["result"]["isError"], true);
    assert_eq!(
        denied["result"]["structuredContent"]["error"]["code"],
        "root_not_allowed"
    );
    assert!(
        !denied["result"]["content"][0]["text"]
            .as_str()
            .unwrap()
            .contains('\n')
    );

    drop(stdin);
    let status = child.wait().unwrap();
    reader.join().unwrap();
    let stderr = std::io::read_to_string(child.stderr.take().unwrap()).unwrap();
    assert!(status.success(), "MCP exited with {status}: {stderr}");
    assert!(stderr.is_empty(), "unexpected MCP stderr: {stderr}");
}

fn send(stdin: &mut impl Write, value: Value) {
    serde_json::to_writer(&mut *stdin, &value).unwrap();
    writeln!(stdin).unwrap();
    stdin.flush().unwrap();
}

fn receive(receiver: &mpsc::Receiver<std::io::Result<String>>, label: &str) -> Value {
    let line = receiver
        .recv_timeout(Duration::from_secs(10))
        .unwrap_or_else(|error| panic!("MCP {label} response timed out: {error}"))
        .unwrap();
    serde_json::from_str(&line).expect("MCP stdout must contain JSON lines")
}
