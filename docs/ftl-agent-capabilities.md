# FTL local agents and computer tools

This feature adds four connected capabilities: multimodal MCP results in FTL,
a native Windows desktop broker, an independent local agent runtime, and bounded
agent teams. The runtime is a separate executable connected through FTL's existing
MCP support. Existing terminal tools and conversation history remain in FTL.

## MCP images in FTL

In Settings → AI → your custom provider, enable **Allow tool screenshots to this
endpoint**. For the selected model, set **Image: On** only after verifying that
the exact served model and server configuration accept image input. Tool support
alone does not establish vision or reliable desktop targeting.

The permission is bound to the saved provider base URL. Changing the destination
withholds tool images until explicitly approved again. Vision inference from a
model name or metadata does not enable tool images. Requests with this permission
disable HTTP proxies and redirects so image delivery remains at that endpoint.
Normal user attachments retain their existing capability behavior.

MCP text and resources become structured JSON. Valid PNG/JPEG/WebP results enter
the model's image input once, after every tool result in the corresponding group.
The inserted observation identifies its tool call and is untrusted tool output,
not a new user instruction. Binary payloads do not appear in text tool results or
routine request logs. Unsupported models and withheld permissions produce an
explicit textual notice. Recent observations take precedence; at most eight
images survive request construction, with four per result, 8 MiB encoded bytes
per image, and 40 million header-declared pixels per image. Invalid payloads are
omitted. Existing conversation persistence retains original typed MCP results;
treat local conversation history as private data and use its normal deletion
controls when retention is unwanted.

## Independent service and teams

See [the service guide](../crates/ftl_agent_service/README.md) for build commands,
MCP configuration, endpoint restrictions, workspace grants and lifecycle.
The service requires an exact model ID and connects only to a literal loopback
HTTP ChatCompletions endpoint. It never downloads models or restarts inference.
Its `agent_start`, status, message, wait, cancel and event tools are available to
FTL through MCP. Service agents can create bounded child agents with independent
contexts and intersected permissions. One model request runs at a time within each service process, which
avoids assuming that a single RTX 5090 benefits from concurrent large-model
requests. FTL main-chat requests and other inference clients are outside this
semaphore; server-side capacity still governs their shared GPU load. Measure
actual throughput before raising concurrency.

The initial service includes read-only workspace tools. It does not automatically
inherit FTL shell tools, implement a PTY host, or provide full terminal parity.
The per-user stdio process has no listening network socket and is not installed
as a Windows system service. Its headless core is separate from the desktop
module; running desktop actions requires an interactive Windows user session.

## Native Windows desktop use

See [the broker guide](../crates/ftl_desktop_broker/README.md). Explicit startup
grants are required for capture, verified model vision, selected monitor and
input. Desktop tools are available only inside service-owned agents, whose
images go to the pinned local model. Direct MCP desktop image-return tools are
withheld because a stdio server cannot verify its client's model destination.
FTL can delegate a desktop task using `agent_start` with the required desktop
tool permissions. The service returns its agent's status and final answer.

Capture/input grants expire, observations have short lifetimes, and each input
requires a matching observation from the same agent. The broker checks monitor
geometry, foreground window and user input before acting and balances synthetic
keys/buttons on cancellation or failure. Native desktop activity must show a
visible Stop control. A single desktop lease prevents two agents from issuing
conflicting actions. Foreground changes between the last check and the OS input
call remain an unavoidable race; the implementation does not claim atomic UI
targeting or control of elevated/secure desktops.

## Validation status

The native Windows app and its tests pass `cargo check`. Focused executed tests
cover the core runtime (8), service (6), desktop broker (18), signed capture
regions (2), and provider image projection (8 in an actual-source harness).
The built service also passes a real MCP subprocess workflow: a parent spawns a
child with narrower permissions, the child reads a synthetic workspace file,
and the parent receives its result through a synthetic loopback model server.

These checks do not demonstrate model vision, coordinate grounding or desktop
task success. No real desktop capture, real input, Stop-window launch, or inference
request to the user's model server was performed. Runtime tests use fake desktop
state, synthetic HTTP servers and temporary workspaces. The provider harness uses
real protocol/image/provider libraries with a small application-container shim;
the complete app test suite was typechecked, not linked and executed.
