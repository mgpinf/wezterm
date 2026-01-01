# `wezterm.docker.get_container(container_id)`

{{since('nightly')}}

Returns detailed information about a specific container, or `nil` if not found.

## Parameters

| Parameter | Type | Description |
|-----------|------|-------------|
| `container_id` | string | Container ID or name |

## Return Value

Returns a table with the following fields:

| Field | Type | Description |
|-------|------|-------------|
| `id` | string | Full container ID |
| `short_id` | string | Short container ID (12 characters) |
| `name` | string | Container name |
| `image` | string | Image name |
| `status` | string | Container status |
| `running` | bool | Whether container is running |
| `paused` | bool | Whether container is paused |
| `pid` | number | Process ID |
| `created` | string | Creation timestamp |
| `started_at` | string | Start timestamp |
| `hostname` | string | Container hostname |
| `working_dir` | string | Working directory |

## Example

```lua
local wezterm = require 'wezterm'

local container = wezterm.docker.get_container 'my-container'
if container then
  wezterm.log_info('Container: ' .. container.name)
  wezterm.log_info('Running: ' .. tostring(container.running))
  wezterm.log_info('Image: ' .. container.image)
end
```
