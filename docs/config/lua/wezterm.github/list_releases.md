# `wezterm.github.list_releases(options)`

{{since('nightly')}}

Returns a list of releases for a repository.

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
| `per_page` | number | nil | Results per page (max 100) |
| `page` | number | nil | Page number |

## Return Value

Returns an array of release tables (see [get_release](get_release.md) for fields),
or a table with an `error` field on failure.

## Example

```lua
local wezterm = require 'wezterm'

local releases = wezterm.github.list_releases {
  owner = 'wez',
  repo = 'wezterm',
  per_page = 5,
}

if releases.error then
  wezterm.log_error('Failed: ' .. releases.error)
else
  for _, release in ipairs(releases) do
    wezterm.log_info(release.tag_name .. ' - ' .. release.name)
  end
end
```
