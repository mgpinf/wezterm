# `wezterm.github.list_pulls(options)`

{{since('nightly')}}

Returns a list of pull requests for a repository.

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
| `state` | string | nil | Filter by state: `open`, `closed`, or `all` |
| `per_page` | number | nil | Results per page (max 100) |
| `page` | number | nil | Page number |

## Return Value

Returns an array of pull request tables (see [get_pull](get_pull.md) for fields),
or a table with an `error` field on failure.

## Example

```lua
local wezterm = require 'wezterm'

local prs = wezterm.github.list_pulls {
  owner = 'wez',
  repo = 'wezterm',
  state = 'open',
  per_page = 5,
}

if prs.error then
  wezterm.log_error('Failed: ' .. prs.error)
else
  for _, pr in ipairs(prs) do
    wezterm.log_info('#' .. pr.number .. ' ' .. pr.title)
  end
end
```
