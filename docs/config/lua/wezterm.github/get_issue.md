# `wezterm.github.get_issue(options)`

{{since('nightly')}}

Returns information about an issue.

## Parameters

| Parameter | Type | Description |
|-----------|------|-------------|
| `options` | table | Required settings |

## Options

| Option | Type | Default | Description |
|--------|------|---------|-------------|
| `owner` | string | required | Repository owner |
| `repo` | string | required | Repository name |
| `number` | number | required | Issue number |
| `token` | string | nil | GitHub personal access token |

## Return Value

Returns a table with the following fields, or a table with an `error` field on failure:

| Field | Type | Description |
|-------|------|-------------|
| `id` | number | Issue ID |
| `number` | number | Issue number |
| `title` | string | Title |
| `state` | string | State (Open, Closed) |
| `body` | string | Description |
| `html_url` | string | Issue URL |
| `user` | string | Author username |
| `labels` | table | Array of label names |
| `assignees` | table | Array of assignee usernames |
| `comments` | number | Number of comments |
| `created_at` | string | Creation timestamp |
| `updated_at` | string | Last update timestamp |
| `closed_at` | string | Close timestamp (if closed) |

## Example

```lua
local wezterm = require 'wezterm'

local issue = wezterm.github.get_issue {
  owner = 'wez',
  repo = 'wezterm',
  number = 100,
}

if issue.error then
  wezterm.log_error('Failed: ' .. issue.error)
else
  wezterm.log_info(issue.title .. ' by ' .. issue.user)
  wezterm.log_info('Labels: ' .. table.concat(issue.labels, ', '))
end
```
