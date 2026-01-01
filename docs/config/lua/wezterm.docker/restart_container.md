# `wezterm.docker.restart_container(container_id [, timeout])`

{{since('nightly')}}

Restarts a container.

## Parameters

| Parameter | Type | Description |
|-----------|------|-------------|
| `container_id` | string | Container ID or name |
| `timeout` | number | Seconds to wait before killing (default: 10) |

## Return Value

Returns `true` on success, or throws an error on failure.

## Example

```lua
local wezterm = require 'wezterm'

wezterm.docker.restart_container 'my-container'
wezterm.log_info 'Container restarted'
```

## Example: With timeout

```lua
local wezterm = require 'wezterm'

wezterm.docker.restart_container('my-container', 30)
```
