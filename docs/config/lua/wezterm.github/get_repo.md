# `wezterm.github.get_repo(options)`

{{since('nightly')}}

Returns information about a repository.

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

## Return Value

Returns a table with the following fields, or a table with an `error` field on failure:

| Field | Type | Description |
|-------|------|-------------|
| `id` | number | Repository ID |
| `name` | string | Repository name |
| `full_name` | string | Full name (owner/repo) |
| `description` | string | Repository description |
| `private` | bool | Whether the repository is private |
| `fork` | bool | Whether the repository is a fork |
| `html_url` | string | Repository URL |
| `clone_url` | string | HTTPS clone URL |
| `ssh_url` | string | SSH clone URL |
| `default_branch` | string | Default branch name |
| `language` | string | Primary language |
| `stargazers_count` | number | Number of stars |
| `forks_count` | number | Number of forks |
| `open_issues_count` | number | Number of open issues |

## Example

```lua
local wezterm = require 'wezterm'

local repo = wezterm.github.get_repo {
  owner = 'wez',
  repo = 'wezterm',
}

if repo.error then
  wezterm.log_error('Failed: ' .. repo.error)
else
  wezterm.log_info(
    repo.full_name .. ' has ' .. repo.stargazers_count .. ' stars'
  )
end
```
