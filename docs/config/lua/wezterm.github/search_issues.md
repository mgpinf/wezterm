# `wezterm.github.search_issues(options)`

{{since('nightly')}}

Searches for issues and pull requests.

## Parameters

| Parameter | Type | Description |
|-----------|------|-------------|
| `options` | table | Required settings |

## Options

| Option | Type | Default | Description |
|--------|------|---------|-------------|
| `query` | string | required | Search query (GitHub search syntax) |
| `token` | string | nil | GitHub personal access token |
| `per_page` | number | nil | Results per page (max 100) |
| `page` | number | nil | Page number |

## Return Value

Returns a table with the following fields, or a table with an `error` field on failure:

| Field | Type | Description |
|-------|------|-------------|
| `total_count` | number | Total number of matching issues |
| `items` | table | Array of issue tables (see [get_issue](get_issue.md)) |

## Example

```lua
local wezterm = require 'wezterm'

local results = wezterm.github.search_issues {
  query = 'repo:wez/wezterm is:issue is:open label:bug',
  per_page = 10,
}

if results.error then
  wezterm.log_error('Failed: ' .. results.error)
else
  wezterm.log_info('Found ' .. results.total_count .. ' issues')
  for _, issue in ipairs(results.items) do
    wezterm.log_info('#' .. issue.number .. ' ' .. issue.title)
  end
end
```
