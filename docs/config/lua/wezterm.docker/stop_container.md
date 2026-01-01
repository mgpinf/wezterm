# `wezterm.docker.stop_container(container_id [, options])`

{{since('nightly')}}

Stops a running container.

## Parameters

| Parameter | Type | Description |
|-----------|------|-------------|
| `container_id` | string | Container ID or name |
| `options` | table | Optional settings |

## Options

| Option | Type | Default | Description |
|--------|------|---------|-------------|
| `timeout` | number | `10` | Seconds to wait before killing |

## Return Value

Returns `true` on success, or throws an error on failure.

## Example

```lua
local wezterm = require 'wezterm'

wezterm.docker.stop_container 'my-container'
wezterm.log_info 'Container stopped'
```

## Example: With timeout

```lua
local wezterm = require 'wezterm'

wezterm.docker.stop_container('my-container', { timeout = 30 })
```
