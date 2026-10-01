mod model;
mod workspace;

use anyhow::{Result, bail};
use clap::Parser;
use ftl_agent_core::{EmptyToolHost, Runtime, ToolHost, permissions, required};
use serde_json::{Value, json};
use std::{
    io::{BufRead, Write},
    path::PathBuf,
    sync::Arc,
};

#[derive(Parser)]
#[command(
    about = "FTL per-user local agent runtime over MCP stdio. No listener or service installation."
)]
struct Args {
    #[arg(long)]
    model: String,
    #[arg(long, default_value = "http://127.0.0.1:8080/v1")]
    endpoint: String,
    #[arg(long)]
    state_dir: PathBuf,
    #[arg(long)]
    vision: bool,
    #[arg(long)]
    allow_desktop_capture: bool,
    #[arg(long)]
    allow_desktop_input: bool,
    #[arg(long)]
    monitor: Option<usize>,
    #[arg(long)]
    workspace: Option<PathBuf>,
}

#[cfg(windows)]
struct DesktopHost(ftl_desktop_broker::DesktopBroker);
#[cfg(windows)]
#[async_trait::async_trait]
impl ToolHost for DesktopHost {
    fn tools(&self) -> Vec<Value> {
        self.0.tool_schemas()
    }
    fn cancel_run(&self, run_id: &str) {
        self.0.cancel_run(run_id);
    }
    fn revoke(&self) {
        self.0.revoke();
    }
    async fn call(&self, run_id: &str, name: &str, args: Value) -> Result<Value> {
        Ok(self.0.call_for_run(run_id, name, args).await?)
    }
}

fn host(args: &Args) -> Result<Arc<dyn ToolHost>> {
    if !args.allow_desktop_capture && !args.allow_desktop_input {
        return Ok(Arc::new(EmptyToolHost));
    }
    #[cfg(windows)]
    {
        let broker =
            ftl_desktop_broker::DesktopBroker::new(ftl_desktop_broker::DesktopBrokerConfig {
                allow_capture: args.allow_desktop_capture,
                allow_input: args.allow_desktop_input,
                model_supports_vision: args.vision,
                approved_model_endpoint: args.endpoint.clone(),
                monitor: args.monitor,
                grant_duration: std::time::Duration::from_secs(300),
            })?;
        Ok(Arc::new(DesktopHost(broker)))
    }
    #[cfg(not(windows))]
    bail!("desktop broker requires an interactive Windows session")
}

fn schema(name: &str, description: &str, properties: Value, required_fields: &[&str]) -> Value {
    json!({"name":name,"description":description,"inputSchema":{"type":"object","properties":properties,"required":required_fields,"additionalProperties":false}})
}

fn service_tools(host: &dyn ToolHost) -> Vec<Value> {
    let mut tools = vec![
        schema(
            "agent_start",
            "Start an independent local agent; returns a run id.",
            json!({"prompt":{"type":"string"},"permissions":{"type":"array","items":{"type":"string"}}}),
            &["prompt"],
        ),
        schema(
            "agent_status",
            "Read local agent status and final output.",
            json!({"run_id":{"type":"string"}}),
            &["run_id"],
        ),
        schema("agent_list", "List persisted local agents.", json!({}), &[]),
        schema(
            "agent_events",
            "Read up to 1000 durable event metadata records after a sequence.",
            json!({"after":{"type":"integer","minimum":0}}),
            &[],
        ),
        schema(
            "agent_send",
            "Send a message between agents in the same team.",
            json!({"from":{"type":"string"},"run_id":{"type":"string"},"message":{"type":"string"}}),
            &["from", "run_id", "message"],
        ),
        schema(
            "agent_wait",
            "Wait up to one minute for a local run.",
            json!({"run_id":{"type":"string"},"timeout_ms":{"type":"integer","minimum":0,"maximum":60000}}),
            &["run_id"],
        ),
        schema(
            "agent_cancel",
            "Cancel a run and its descendants.",
            json!({"run_id":{"type":"string"}}),
            &["run_id"],
        ),
    ];
    tools.extend(host.tools().into_iter().filter(|tool| {
        matches!(
            tool["name"].as_str(),
            Some("workspace_read" | "workspace_list")
        )
    }));
    tools
}

async fn call(
    runtime: &Arc<Runtime>,
    host: &dyn ToolHost,
    name: &str,
    args: Value,
) -> Result<Value> {
    let value = match name {
        "agent_start" => {
            json!(runtime.start(required(&args, "prompt")?.into(), None, permissions(&args)?)?)
        }
        "agent_status" => json!(runtime.status(required(&args, "run_id")?)?),
        "agent_list" => json!(runtime.list()),
        "agent_events" => json!(runtime.events(args["after"].as_u64().unwrap_or(0))),
        "agent_send" => {
            runtime.send(
                required(&args, "from")?,
                required(&args, "run_id")?,
                required(&args, "message")?.into(),
            )?;
            json!({"sent":true})
        }
        "agent_wait" => json!(
            runtime
                .wait(
                    required(&args, "run_id")?,
                    args["timeout_ms"].as_u64().unwrap_or(1000)
                )
                .await?
        ),
        "agent_cancel" => {
            runtime.cancel(required(&args, "run_id")?)?;
            json!({"cancelled":true})
        }
        "workspace_read" | "workspace_list" => {
            return host.call("mcp-interactive-client", name, args).await;
        }
        _ => bail!("tool is not exposed to MCP clients"),
    };
    Ok(json!({"content":[{"type":"text","text":serde_json::to_string(&value)?}],"isError":false}))
}

async fn dispatch(runtime: &Arc<Runtime>, host: &dyn ToolHost, request: Value) -> Value {
    let id = request["id"].clone();
    let method = request["method"].as_str().unwrap_or("");
    let result = match method {
        "initialize" => Ok(
            json!({"protocolVersion":"2024-11-05","capabilities":{"tools":{}},"serverInfo":{"name":"ftl-agent-service","version":env!("CARGO_PKG_VERSION")}}),
        ),
        "ping" => Ok(json!({})),
        "tools/list" => Ok(json!({"tools":service_tools(host)})),
        "tools/call" => match required(&request["params"], "name") {
            Ok(name) => call(runtime, host, name, request["params"]["arguments"].clone()).await,
            Err(e) => Err(e),
        },
        _ => {
            return json!({"jsonrpc":"2.0","id":id,"error":{"code":-32601,"message":"Method not found"}});
        }
    };
    match result {
        Ok(result) => json!({"jsonrpc":"2.0","id":id,"result":result}),
        Err(_) => {
            json!({"jsonrpc":"2.0","id":id,"result":{"content":[{"type":"text","text":"Request failed: invalid arguments, permission denied, capacity reached, or unavailable run/tool."}],"isError":true}})
        }
    }
}

fn read_frame(reader: &mut impl BufRead) -> std::io::Result<Option<Vec<u8>>> {
    let mut frame = Vec::new();
    loop {
        let buffer = reader.fill_buf()?;
        if buffer.is_empty() {
            return if frame.is_empty() {
                Ok(None)
            } else {
                Err(std::io::Error::new(
                    std::io::ErrorKind::UnexpectedEof,
                    "incomplete frame",
                ))
            };
        }
        let len = buffer
            .iter()
            .position(|b| *b == b'\n')
            .map(|i| i + 1)
            .unwrap_or(buffer.len());
        if frame.len() + len > 1024 * 1024 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "frame too large",
            ));
        }
        let done = buffer[len - 1] == b'\n';
        frame.extend_from_slice(&buffer[..len]);
        reader.consume(len);
        if done {
            return Ok(Some(frame));
        }
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();
    model::validate_endpoint(&args.endpoint)?;
    let model = Arc::new(model::LocalModel::new(&args.endpoint, args.model.clone())?);
    let host: Arc<dyn ToolHost> = Arc::new(workspace::WorkspaceHost::new(
        args.workspace.clone(),
        host(&args)?,
    )?);
    let runtime = Runtime::open(&args.state_dir, model, host.clone(), args.vision)?;
    let (tx, mut rx) = tokio::sync::mpsc::channel(16);
    std::thread::spawn(move || {
        let stdin = std::io::stdin();
        let mut reader = stdin.lock();
        while let Ok(Some(frame)) = read_frame(&mut reader) {
            if tx.blocking_send(frame).is_err() {
                break;
            }
        }
    });
    let pending = Arc::new(tokio::sync::Semaphore::new(16));
    let (output, mut output_rx) = tokio::sync::mpsc::channel(16);
    std::thread::spawn(move || {
        while let Some(response) = output_rx.blocking_recv() {
            if write_response(response).is_err() {
                break;
            }
        }
    });
    let mut requests = tokio::task::JoinSet::new();
    loop {
        let frame = tokio::select! {frame=rx.recv()=>frame,_=tokio::signal::ctrl_c()=>None};
        let Some(frame) = frame else { break };
        let request: Value = match serde_json::from_slice(&frame) {
            Ok(v) => v,
            Err(_) => continue,
        };
        if request.get("id").is_none() {
            continue;
        }
        let permit = match pending.clone().try_acquire_owned() {
            Ok(p) => p,
            Err(_) => {
                if output.try_send(json!({"jsonrpc":"2.0","id":request["id"],"error":{"code":-32000,"message":"Too many pending requests"}})).is_err() {
                    break;
                }
                continue;
            }
        };
        let runtime = runtime.clone();
        let host = host.clone();
        let output = output.clone();
        requests.spawn(async move {
            let _permit = permit;
            let result = dispatch(&runtime, host.as_ref(), request).await;
            let _ = output.send(result).await;
        });
        while requests.try_join_next().is_some() {}
    }
    runtime.begin_shutdown();
    for run in runtime.list() {
        let _ = runtime.cancel(&run.id);
    }
    host.cancel_run("mcp-interactive-client");
    requests.abort_all();
    runtime.shutdown().await;
    Ok(())
}

fn write_response(value: Value) -> Result<()> {
    let stdout = std::io::stdout();
    let mut stdout = stdout.lock();
    serde_json::to_writer(&mut stdout, &value)?;
    stdout.write_all(b"\n")?;
    stdout.flush()?;
    Ok(())
}

#[cfg(test)]
#[path = "main_tests.rs"]
mod tests;
