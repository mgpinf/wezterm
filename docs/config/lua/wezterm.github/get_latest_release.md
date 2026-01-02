# `wezterm.github.get_latest_release(options)`

{{since('nightly')}}

Returns the latest published release for a repository.

## Parameters

| Parameter | Type | Description |
|-----------|------|-------------|
| `options` | table | Required settings |

## Options

| Option | Type | Default | Description |
|--------|------|---------|-------------|
| `owner` | string | required | Repository owner |
| `repo` | string | required | Repository name |
| `token` | string | nil | GitHub personal access token |

## Return Value

Returns a release table (see [get_release](get_release.md) for fields),
or a table with an `error` field on failure.

## Example

```lua
local wezterm = require 'wezterm'

local release = wezterm.github.get_latest_release {
  owner = 'wez',
  repo = 'wezterm',
}

if release.error then
  wezterm.log_error('Failed: ' .. release.error)
else
  wezterm.log_info('Latest: ' .. release.tag_name)
end
```
