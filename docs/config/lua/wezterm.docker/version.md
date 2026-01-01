# `wezterm.docker.version()`

{{since('nightly')}}

Returns Docker version information, or `nil` if Docker is not available.

## Return Value

Returns a table with the following fields:

| Field | Type | Description |
|-------|------|-------------|
| `version` | string | Docker version |
| `api_version` | string | API version |
| `os` | string | Operating system |
| `arch` | string | Architecture |
| `kernel_version` | string | Kernel version |
| `git_commit` | string | Git commit hash |
| `go_version` | string | Go version used to build Docker |

## Example

```lua
local wezterm = require 'wezterm'

local version = wezterm.docker.version()
if version then
  wezterm.log_info('Docker version: ' .. version.version)
  wezterm.log_info('API version: ' .. version.api_version)
end
```
