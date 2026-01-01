# `wezterm.docker.list_containers([options])`

{{since('nightly')}}

Returns a list of Docker containers, or `nil` if Docker is not available.

## Parameters

| Parameter | Type | Description |
|-----------|------|-------------|
| `options` | table | Optional settings |

## Options

| Option | Type | Default | Description |
|--------|------|---------|-------------|
| `all` | bool | `true` | Include stopped containers |
| `limit` | number | nil | Maximum number of containers to return |

## Return Value

Returns an array of tables, each with the following fields:

| Field | Type | Description |
|-------|------|-------------|
| `id` | string | Full container ID |
| `short_id` | string | Short container ID (12 characters) |
| `names` | table | Array of container names |
| `image` | string | Image name |
| `state` | string | Container state (Running, Exited, etc.) |
| `status` | string | Human-readable status |
| `created` | number | Creation timestamp |

## Example

```lua
local wezterm = require 'wezterm'

local containers = wezterm.docker.list_containers()
if containers then
  for _, container in ipairs(containers) do
    wezterm.log_info(container.names[1] .. ' - ' .. container.state)
  end
end
```

## Example: Only running containers

```lua
local wezterm = require 'wezterm'

local containers = wezterm.docker.list_containers { all = false }
if containers then
  wezterm.log_info('Running containers: ' .. #containers)
end
```
