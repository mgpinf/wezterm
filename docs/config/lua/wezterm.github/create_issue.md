# `wezterm.github.create_issue(options)`

{{since('nightly')}}

Creates a new issue.

## Parameters

| Parameter | Type | Description |
|-----------|------|-------------|
| `options` | table | Required settings |

## Options

| Option | Type | Default | Description |
|--------|------|---------|-------------|
| `owner` | string | required | Repository owner |
| `repo` | string | required | Repository name |
| `title` | string | required | Issue title |
| `token` | string | nil | GitHub personal access token |
| `body` | string | nil | Issue description |
| `labels` | table | nil | Array of label names |
| `assignees` | table | nil | Array of assignee usernames |

## Return Value

Returns the created issue table (see [get_issue](get_issue.md) for fields),
or a table with an `error` field on failure.

## Example

```lua
local wezterm = require 'wezterm'

local token = os.getenv 'GITHUB_TOKEN'
local issue = wezterm.github.create_issue {
  owner = 'myuser',
  repo = 'myrepo',
  title = 'Bug: Something is broken',
  body = 'Steps to reproduce...',
  labels = { 'bug' },
  token = token,
}

if issue.error then
  wezterm.log_error('Failed: ' .. issue.error)
else
  wezterm.log_info(
    'Created issue #' .. issue.number .. ': ' .. issue.html_url
  )
end
```
