# `wezterm.docker.start_container(container_id)`

{{since('nightly')}}

Starts a stopped container.

## Parameters

| Parameter | Type | Description |
|-----------|------|-------------|
| `container_id` | string | Container ID or name |

## Return Value

Returns `true` on success, or throws an error on failure.

## Example

```lua
local wezterm = require 'wezterm'

wezterm.docker.start_container 'my-container'
wezterm.log_info 'Container started'
```
