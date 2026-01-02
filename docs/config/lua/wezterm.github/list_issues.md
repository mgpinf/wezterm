# `wezterm.github.list_issues(options)`

{{since('nightly')}}

Returns a list of issues for a repository.

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
| `labels` | table | nil | Filter by labels (array of strings) |
| `per_page` | number | nil | Results per page (max 100) |
| `page` | number | nil | Page number |

## Return Value

Returns an array of issue tables (see [get_issue](get_issue.md) for fields),
or a table with an `error` field on failure.

## Example

```lua
local wezterm = require 'wezterm'

local issues = wezterm.github.list_issues {
  owner = 'wez',
  repo = 'wezterm',
  state = 'open',
  labels = { 'bug' },
  per_page = 10,
}

if issues.error then
  wezterm.log_error('Failed: ' .. issues.error)
else
  for _, issue in ipairs(issues) do
    wezterm.log_info('#' .. issue.number .. ' ' .. issue.title)
  end
end
```
