# `wezterm.github.create_pull(options)`

{{since('nightly')}}

Creates a new pull request.

## Parameters

| Parameter | Type | Description |
|-----------|------|-------------|
| `options` | table | Required settings |

## Options

| Option | Type | Default | Description |
|--------|------|---------|-------------|
| `owner` | string | required | Repository owner |
| `repo` | string | required | Repository name |
| `title` | string | required | Pull request title |
| `head` | string | required | Head branch name |
| `base` | string | required | Base branch name |
| `token` | string | nil | GitHub personal access token |
| `body` | string | nil | Pull request description |
| `draft` | bool | false | Create as draft |

## Return Value

Returns the created pull request table (see [get_pull](get_pull.md) for fields),
or a table with an `error` field on failure.

## Example

```lua
local wezterm = require 'wezterm'

local token = os.getenv 'GITHUB_TOKEN'
local pr = wezterm.github.create_pull {
  owner = 'myuser',
  repo = 'myrepo',
  title = 'Add new feature',
  head = 'feature-branch',
  base = 'main',
  body = 'This PR adds a new feature.',
  token = token,
}

if pr.error then
  wezterm.log_error('Failed: ' .. pr.error)
else
  wezterm.log_info('Created PR #' .. pr.number .. ': ' .. pr.html_url)
end
```
