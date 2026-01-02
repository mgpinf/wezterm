# `wezterm.github.list_branches(options)`

{{since('nightly')}}

Returns a list of branches for a repository.

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

Returns an array of tables with the following fields, or a table with an `error` field on failure:

| Field | Type | Description |
|-------|------|-------------|
| `name` | string | Branch name |
| `sha` | string | Commit SHA at branch tip |
| `protected` | bool | Whether the branch is protected |

## Example

```lua
local wezterm = require 'wezterm'

local branches = wezterm.github.list_branches {
  owner = 'wez',
  repo = 'wezterm',
}

if branches.error then
  wezterm.log_error('Failed: ' .. branches.error)
else
  for _, branch in ipairs(branches) do
    local protected = branch.protected and ' (protected)' or ''
    wezterm.log_info(branch.name .. protected)
  end
end
```
