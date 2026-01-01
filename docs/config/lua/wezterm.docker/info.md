# `wezterm.docker.info()`

{{since('nightly')}}

Returns Docker system information, or `nil` if Docker is not available.

## Return Value

Returns a table with the following fields:

| Field | Type | Description |
|-------|------|-------------|
| `containers` | number | Total number of containers |
| `containers_running` | number | Number of running containers |
| `containers_paused` | number | Number of paused containers |
| `containers_stopped` | number | Number of stopped containers |
| `images` | number | Total number of images |
| `name` | string | Docker host name |
| `operating_system` | string | Operating system |
| `os_type` | string | OS type |
| `architecture` | string | Architecture |
| `cpus` | number | Number of CPUs |
| `memory_total` | number | Total memory in bytes |

## Example

```lua
local wezterm = require 'wezterm'

local info = wezterm.docker.info()
if info then
  wezterm.log_info('Running containers: ' .. info.containers_running)
  wezterm.log_info('Total images: ' .. info.images)
end
```
