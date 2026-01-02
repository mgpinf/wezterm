# `wezterm.github.get_commit(options)`

{{since('nightly')}}

Returns information about a commit.

## Parameters

| Parameter | Type | Description |
|-----------|------|-------------|
| `options` | table | Required settings |

## Options

| Option | Type | Default | Description |
|--------|------|---------|-------------|
| `owner` | string | required | Repository owner |
| `repo` | string | required | Repository name |
| `sha` | string | required | Commit SHA |
| `token` | string | nil | GitHub personal access token |

## Return Value

Returns a table with the following fields, or a table with an `error` field on failure:

| Field | Type | Description |
|-------|------|-------------|
| `sha` | string | Full commit SHA |
| `message` | string | Commit message |
| `author_name` | string | Author name |
| `author_email` | string | Author email |
| `author_login` | string | Author GitHub username |
| `html_url` | string | Commit URL |
| `additions` | number | Lines added |
| `deletions` | number | Lines deleted |
| `total` | number | Total lines changed |

## Example

```lua
local wezterm = require 'wezterm'

local commit = wezterm.github.get_commit {
  owner = 'wez',
  repo = 'wezterm',
  sha = 'abc1234',
}

if commit.error then
  wezterm.log_error('Failed: ' .. commit.error)
else
  wezterm.log_info(commit.author_name .. ': ' .. commit.message)
  wezterm.log_info('+' .. commit.additions .. ' -' .. commit.deletions)
end
```
