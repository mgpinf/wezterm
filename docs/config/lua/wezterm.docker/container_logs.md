# `wezterm.docker.container_logs(container_id [, options])`

{{since('nightly')}}

Returns the logs from a container.

## Parameters

| Parameter | Type | Description |
|-----------|------|-------------|
| `container_id` | string | Container ID or name |
| `options` | table | Optional settings |

## Options

| Option | Type | Default | Description |
|--------|------|---------|-------------|
| `stdout` | bool | `true` | Include stdout |
| `stderr` | bool | `true` | Include stderr |
| `tail` | string | `"all"` | Number of lines from end (e.g., `"100"`) or `"all"` |
| `since` | number | `0` | Unix timestamp - only logs after this time |
| `timestamps` | bool | `false` | Prefix each line with timestamp |

## Return Value

Returns the logs as a string, or throws an error on failure.

## Example

```lua
local wezterm = require 'wezterm'

local logs = wezterm.docker.container_logs 'my-container'
wezterm.log_info(logs)
```

## Example: Last 50 lines with timestamps

```lua
local wezterm = require 'wezterm'

local logs = wezterm.docker.container_logs('my-container', {
  tail = '50',
  timestamps = true,
})
```

## Example: Only stderr

```lua
local wezterm = require 'wezterm'

local logs = wezterm.docker.container_logs('my-container', {
  stdout = false,
  stderr = true,
})
```
