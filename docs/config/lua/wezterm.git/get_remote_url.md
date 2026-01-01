# `wezterm.git.get_remote_url(path [, remote_name])`

{{since('nightly')}}

Returns the URL of a remote, or `nil` if not in a git repository or the remote
doesn't exist.

## Parameters

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `path` | string | | Path inside the repository |
| `remote_name` | string | `"origin"` | Name of the remote |

## Example

```lua
local wezterm = require 'wezterm'

-- Get origin URL
local url = wezterm.git.get_remote_url '/path/to/repo'
-- e.g., "git@github.com:user/repo.git"

-- Get specific remote
local upstream = wezterm.git.get_remote_url('/path/to/repo', 'upstream')
```
