# FTL local agent service

This executable runs local agents without the FTL UI runtime. It speaks MCP
JSON-RPC over newline-delimited stdio; it does not open a TCP listener or install
a Windows service. An MCP client owns the stdio connection and its lifetime;
closing that connection stops the agents. This is not a detached background daemon.

Build with `cargo build -p ftl_agent_service --bin ftl-agent-service`. Use the
resulting executable in FTL's MCP configuration. The FTL Windows installer also
places this executable beside `ftl.exe` in the chosen installation directory.
It does not start it or register a Windows service. For example:

```json
{
  "mcpServers": {
    "ftl-local-agents": {
      "command": "C:\\path\\to\\ftl-agent-service.exe",
      "args": [
        "--model", "EXACT_MODEL_ID_FROM_YOUR_SERVER",
        "--endpoint", "http://127.0.0.1:8080/v1",
        "--state-dir", "C:\\Users\\YOUR_USER\\AppData\\Local\\FTL\\agent-state",
        "--workspace", "C:\\src\\YOUR_WORKSPACE"
      ]
    }
  }
}
```

The model argument is mandatory. The service never downloads a model, starts the
model server, changes server configuration, signs in, or falls back to a cloud
provider. Only literal loopback HTTP destinations with `/v1` are accepted;
proxies and redirects are disabled. This constrains this process's inference
connection, not the separate model server's own network behavior.

`agent_start` accepts a prompt and optional allowed tool-name array, then returns
a run ID immediately. `agent_status`, `agent_wait`, `agent_list`, `agent_events`,
and `agent_cancel` inspect or manage runs. `agent_send` delivers a durable message
between members of the same team. Models receive `agent_spawn`, `agent_send`,
`agent_wait`, and `agent_cancel`; child contexts are independent, tool grants are
intersected with their parent's grants, and a model can cancel only its own
children. Human MCP clients can manage all runs in their owned service process.
Peer messages are explicitly marked as untrusted observations.

At most four agents are admitted at once, including agents finishing cancellation.
Delegation depth is two, each agent gets 32 model rounds, and only one inference
request runs at a time. Waiting parents release the inference slot. Each model
request has a 180-second deadline and an 8 MiB response limit. A service state
directory holds at most 256 run records and a 64 MiB journal; archive it and choose
a new directory when full. Event polling returns at most 1000 records per call.

`--workspace` grants read-only `workspace_read` and `workspace_list` to service
agents and the MCP client. Paths must be relative and resolve within that root;
symlink escapes are rejected. Files must be regular UTF-8 text of at most 64 KiB,
and listings are limited to 1000 entries. This is a read boundary, not an OS
sandbox against another local process racing filesystem changes. Omit the flag
to omit these tools. The service does not implement shell execution, file writes,
PTY streaming, or automatically inherit FTL's existing application tools.

For the optional native Windows desktop host, add **all applicable explicit
startup grants**:

```text
--vision --allow-desktop-capture --monitor 0
```

Add `--allow-desktop-input` only when capture and input are authorized. The broker
requires an interactive Windows session and grants expire after five minutes.
The headless runtime is separate from the broker module; this executable hosts
that broker in the same interactive process. Session-0 Windows service hosting
and a separate authenticated broker transport are not implemented. Grants cannot
be renewed by a model; restart with human-approved flags for a new grant. Do not
enable desktop tools for an endpoint/model whose image support is unverified.

Desktop tools are available only to service-owned agents started with
`agent_start`. They are deliberately absent from the MCP client's direct tool
list: MCP cannot prove where that client would forward an image. For example,
start an agent with a task prompt and `permissions` containing `desktop_observe`
and the input tools that task needs. Screenshots stay in that agent's memory and
go only to the explicitly configured loopback model. The MCP client receives
status and the agent's final textual answer. Generic MCP image support in FTL
is a separate feature for independently approved MCP servers.

MCP images retain typed content. The runtime projects each completed tool-call
group as tool-role text results followed by a user-role image observation tagged
with its originating call ID. Images are withheld without `--vision`. No request
bodies, screenshots, key input, or model output are written to routine logs.

The state journal intentionally persists final answers and peer messages so
clients can recover results and mailbox history. Treat the state directory as
private user data. It contains no prompts, model request histories, or screenshots.
A process lock prevents multiple writers. Startup marks in-flight runs interrupted;
it never replays an uncertain tool action. Corrupt or oversized journals fail
closed. Cancellation signals and broker revocation still run if persistence fails.

Disconnecting stdin or pressing Ctrl+C cancels active runs and waits for bounded
in-flight tool actions to finish balanced input cleanup. The existing desktop
grant, not model text, authorizes capture/input. Ordinary startup without those
flags neither captures the desktop nor controls it.

This is an initial independent agent runtime with team orchestration, approved
workspace reads, and an optional native desktop host. It does not claim full
FTL terminal/UI feature parity or replace the application's conversation store.
