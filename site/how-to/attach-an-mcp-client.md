# How to attach an MCP client

<!-- Template: Good Docs Project how-to (CC-BY 4.0) — https://github.com/thegooddocsproject/templates -->

## Goal

Connect an AI agent to Codefang over the Model Context Protocol (MCP) so the
agent can run code analysis as a tool.

## Prerequisites

- `codefang` installed and on your `PATH`.
- An MCP-compatible client (for example, Claude Code or another agent that
  launches MCP servers over stdio).
- For the `codefang_history` tool, an absolute path to a local Git repository.

## Steps

1. Confirm the MCP server starts. It communicates over stdio using JSON-RPC, the
   standard transport for local MCP tool use:

   ```bash
   codefang mcp
   ```

   Press `Ctrl-C` to stop it — the agent will launch its own copy.

2. Register Codefang as an MCP server in your client. For Claude Code, add it to
   `~/.claude/settings.json` (user scope) or `.claude/settings.json` (project
   scope):

   ```json
   {
     "mcpServers": {
       "codefang": {
         "command": "codefang",
         "args": ["mcp"],
         "env": {}
       }
     }
   }
   ```

3. Restart the client so it picks up the new server. The agent then discovers
   three tools via the standard MCP `tools/list` method: `codefang_analyze`
   (static analysis on inline code), `uast_parse` (parse source into a UAST),
   and `codefang_history` (history analysis on a local repo).

4. Ask the agent to run history analysis. The `codefang_history` tool requires
   an absolute `repo_path`; a relative path is rejected:

   ```json
   {
     "name": "codefang_history",
     "arguments": {
       "repo_path": "/home/user/projects/myapp",
       "analyzers": ["burndown", "devs"],
       "limit": 500
     }
   }
   ```

5. If a call misbehaves, enable debug logging to stderr by adding the `--debug`
   argument to the server registration:

   ```json
   {
     "mcpServers": {
       "codefang": {
         "command": "codefang",
         "args": ["mcp", "--debug"],
         "env": {}
       }
     }
   }
   ```

## Result

The agent lists the three Codefang tools and returns structured analysis when it
calls them. A successful `codefang_analyze` or `codefang_history` call that comes
back without `isError: true` confirms the client is attached.

## See also

- [MCP server reference](../integrations/mcp.md)
- [AI agents](../integrations/ai-agents.md)
- [How to read a burndown chart](read-a-burndown-chart.md)
