# `wezterm.github.get_pull(options)`

{{since('nightly')}}

Returns information about a pull request.

## Parameters

| Parameter | Type | Description |
|-----------|------|-------------|
| `options` | table | Required settings |

## Options

| Option | Type | Default | Description |
|--------|------|---------|-------------|
| `owner` | string | required | Repository owner |
| `repo` | string | required | Repository name |
| `number` | number | required | Pull request number |
| `token` | string | nil | GitHub personal access token |

## Return Value

Returns a table with the following fields, or a table with an `error` field on failure:

| Field | Type | Description |
|-------|------|-------------|
| `id` | number | Pull request ID |
| `number` | number | Pull request number |
| `title` | string | Title |
| `state` | string | State (Open, Closed) |
| `body` | string | Description |
| `html_url` | string | Pull request URL |
| `user` | string | Author username |
| `head_ref` | string | Head branch name |
| `base_ref` | string | Base branch name |
| `draft` | bool | Whether it's a draft |
| `merged_at` | string | Merge timestamp (if merged) |
| `mergeable` | bool | Whether it can be merged |
| `commits` | number | Number of commits |
| `additions` | number | Lines added |
| `deletions` | number | Lines deleted |
| `changed_files` | number | Number of changed files |

## Example

```lua
local wezterm = require 'wezterm'

local pr = wezterm.github.get_pull {
  owner = 'wez',
  repo = 'wezterm',
  number = 123,
}

if pr.error then
  wezterm.log_error('Failed: ' .. pr.error)
else
  wezterm.log_info(pr.title .. ' by ' .. pr.user)
  wezterm.log_info('+' .. pr.additions .. ' -' .. pr.deletions)
end
```
