# `wezterm.github.update_issue(options)`

{{since('nightly')}}

Updates an existing issue.

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
| `title` | string | nil | New title |
| `body` | string | nil | New description |
| `state` | string | nil | New state: `open` or `closed` |
| `labels` | table | nil | New labels (replaces existing) |
| `assignees` | table | nil | New assignees (replaces existing) |

## Return Value

Returns the updated issue table (see [get_issue](get_issue.md) for fields),
or a table with an `error` field on failure.

## Example

```lua
local wezterm = require 'wezterm'

local token = os.getenv 'GITHUB_TOKEN'
local issue = wezterm.github.update_issue {
  owner = 'myuser',
  repo = 'myrepo',
  number = 42,
  state = 'closed',
  labels = { 'wontfix' },
  token = token,
}

if issue.error then
  wezterm.log_error('Failed: ' .. issue.error)
else
  wezterm.log_info('Issue closed: ' .. issue.html_url)
end
```
