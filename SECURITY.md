# Security Policy

## Deployment Security

### REST API Server (`noricum serve`)

- **Do NOT expose the API server to the public internet.** It is designed for local development and internal use behind a reverse proxy.
- Set `NORICUM_API_KEY` to require authentication on all endpoints.
- Set `NORICUM_CORS_ORIGINS` to restrict cross-origin access (comma-separated list of allowed origins). Defaults to `localhost` only.
- The API enforces a 10 MB maximum payload size on all source code inputs.

### MCP Server (`noricum-mcp-server`)

- The MCP server communicates over stdio (stdin/stdout) and is intended for use with local IDE integrations (e.g., Claude Code).
- All tool inputs are validated for size (10 MB limit).
- Migration calls have a 5-minute timeout to prevent runaway processes.

## LLM Data Handling

- **Source code is sent to LLM providers** (Anthropic Claude API) as part of analysis, translation, repair, and test generation prompts.
- If you are migrating proprietary or sensitive code, ensure your organization's data handling policies permit sending source code to third-party APIs.
- Use `--no-llm` mode for fully offline, rule-based migration that never contacts external services.
- The Ollama integration (when available) runs models locally with no external data transmission.

## LLM Agent File Access

- The rig-rs tool implementations (`ReadSourceTool`, `WriteSourceTool`) restrict file access:
  - Path traversal (`..`) is rejected.
  - Absolute paths are restricted to `/tmp`, `/var/tmp`, and the current working directory.
- These restrictions prevent LLM agents from reading or writing arbitrary files on the host.

## Threat Model

| Threat | Mitigation |
|--------|-----------|
| LLM prompt injection via C source | Agents operate in a sandboxed pipeline; file I/O is path-restricted |
| Arbitrary file read/write by agents | Path validation rejects traversal and out-of-scope absolute paths |
| Unauthenticated API access | Optional `NORICUM_API_KEY` env var for Bearer/header auth |
| Large payload DoS | 10 MB input size limit on API and MCP servers |
| Runaway migration process | 5-minute timeout on MCP async migration calls |
| CORS abuse | Restrictive CORS (localhost-only default, env-configurable allowlist) |
| Source code exfiltration via LLM | Use `--no-llm` for sensitive code; review Anthropic data policies |

## Reporting Vulnerabilities

If you discover a security vulnerability, please open a private issue at:
https://github.com/JuanMarchetto/noricum/issues

Or email: juanmarchetto@users.noreply.github.com
