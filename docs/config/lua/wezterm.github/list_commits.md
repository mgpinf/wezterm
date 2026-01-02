# `wezterm.github.list_commits(options)`

{{since('nightly')}}

Returns a list of commits for a repository.

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
| `sha` | string | nil | Branch name or commit SHA to start from |
| `per_page` | number | nil | Results per page (max 100) |
| `page` | number | nil | Page number |

## Return Value

Returns an array of commit tables (see [get_commit](get_commit.md) for fields),
or a table with an `error` field on failure.

## Example

```lua
local wezterm = require 'wezterm'

local commits = wezterm.github.list_commits {
  owner = 'wez',
  repo = 'wezterm',
  sha = 'main',
  per_page = 5,
}

if commits.error then
  wezterm.log_error('Failed: ' .. commits.error)
else
  for _, commit in ipairs(commits) do
    wezterm.log_info(
      commit.sha:sub(1, 7) .. ' ' .. commit.message:match '[^\n]+'
    )
  end
end
```
