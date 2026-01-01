# `wezterm.docker.create_container(options)`

{{since('nightly')}}

Creates a new container and returns its ID.

## Parameters

| Parameter | Type | Description |
|-----------|------|-------------|
| `options` | table | Container configuration |

## Options

| Option | Type | Default | Description |
|--------|------|---------|-------------|
| `image` | string | required | Image to use |
| `name` | string | nil | Container name |
| `cmd` | table | nil | Command to run (array of strings) |
| `env` | table | nil | Environment variables (array of "KEY=value" strings) |
| `working_dir` | string | nil | Working directory |
| `hostname` | string | nil | Container hostname |
| `tty` | bool | `false` | Allocate a TTY |
| `attach_stdin` | bool | `false` | Attach stdin |
| `attach_stdout` | bool | `false` | Attach stdout |
| `attach_stderr` | bool | `false` | Attach stderr |
| `open_stdin` | bool | `true` | Keep stdin open |

## Return Value

Returns the container ID as a string, or throws an error on failure.

## Example

```lua
local wezterm = require 'wezterm'

local container_id = wezterm.docker.create_container {
  image = 'ubuntu:latest',
  name = 'my-ubuntu',
  cmd = { '/bin/bash' },
  tty = true,
}
wezterm.log_info('Created container: ' .. container_id)
```

## Example: With environment variables

```lua
local wezterm = require 'wezterm'

local container_id = wezterm.docker.create_container {
  image = 'node:18',
  name = 'my-node-app',
  cmd = { 'node', 'app.js' },
  env = { 'NODE_ENV=production', 'PORT=3000' },
  working_dir = '/app',
}
```
