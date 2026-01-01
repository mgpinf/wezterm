# `wezterm.docker.exec(container_id, options)`

{{since('nightly')}}

Executes a command in a running container.

## Parameters

| Parameter | Type | Description |
|-----------|------|-------------|
| `container_id` | string | Container ID or name |
| `options` | table | Execution configuration |

## Options

| Option | Type | Default | Description |
|--------|------|---------|-------------|
| `cmd` | table | required | Command to run (array of strings) |
| `working_dir` | string | nil | Working directory |
| `env` | table | nil | Environment variables (array of "KEY=value" strings) |
| `attach_stdout` | bool | `true` | Capture stdout |
| `attach_stderr` | bool | `true` | Capture stderr |
| `tty` | bool | `false` | Allocate a TTY |
| `privileged` | bool | `false` | Run in privileged mode |
| `user` | string | nil | User to run as |

## Return Value

Returns a table with the following fields:

| Field | Type | Description |
|-------|------|-------------|
| `stdout` | string | Standard output |
| `stderr` | string | Standard error |
| `exit_code` | number | Exit code |

## Example

```lua
local wezterm = require 'wezterm'

local result = wezterm.docker.exec('my-container', {
  cmd = { 'ls', '-la', '/app' },
})
wezterm.log_info('Output: ' .. result.stdout)
wezterm.log_info('Exit code: ' .. result.exit_code)
```

## Example: With environment variables

```lua
local wezterm = require 'wezterm'

local result = wezterm.docker.exec('my-container', {
  cmd = { 'npm', 'test' },
  working_dir = '/app',
  env = { 'NODE_ENV=test' },
})

if result.exit_code == 0 then
  wezterm.log_info 'Tests passed'
else
  wezterm.log_error('Tests failed: ' .. result.stderr)
end
```

## Example: Run as specific user

```lua
local wezterm = require 'wezterm'

local result = wezterm.docker.exec('my-container', {
  cmd = { 'whoami' },
  user = 'root',
})
```
