# `wezterm.github.merge_pull(options)`

{{since('nightly')}}

Merges a pull request.

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
| `commit_title` | string | nil | Custom merge commit title |
| `commit_message` | string | nil | Custom merge commit message |
| `merge_method` | string | nil | Merge method: `merge`, `squash`, or `rebase` |

## Return Value

Returns a table with the following fields, or a table with an `error` field on failure:

| Field | Type | Description |
|-------|------|-------------|
| `sha` | string | Merge commit SHA |
| `merged` | bool | Whether the merge succeeded |
| `message` | string | Status message |

## Example

```lua
local wezterm = require 'wezterm'

local token = os.getenv 'GITHUB_TOKEN'
local result = wezterm.github.merge_pull {
  owner = 'myuser',
  repo = 'myrepo',
  number = 42,
  merge_method = 'squash',
  token = token,
}

if result.error then
  wezterm.log_error('Failed: ' .. result.error)
elseif result.merged then
  wezterm.log_info('Merged! SHA: ' .. result.sha)
end
```
