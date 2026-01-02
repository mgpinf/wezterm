# `wezterm.github.list_repos([options])`

{{since('nightly')}}

Returns a list of repositories for the authenticated user.

## Parameters

| Parameter | Type | Description |
|-----------|------|-------------|
| `options` | table | Optional settings |

## Options

| Option | Type | Default | Description |
|--------|------|---------|-------------|
| `token` | string | nil | GitHub personal access token |
| `per_page` | number | nil | Results per page (max 100) |
| `page` | number | nil | Page number |

## Return Value

Returns an array of repository tables (see [get_repo](get_repo.md) for fields),
or a table with an `error` field on failure.

## Example

```lua
local wezterm = require 'wezterm'

local token = os.getenv 'GITHUB_TOKEN'
local repos = wezterm.github.list_repos { token = token, per_page = 10 }

if repos.error then
  wezterm.log_error('Failed: ' .. repos.error)
else
  for _, repo in ipairs(repos) do
    wezterm.log_info(repo.full_name)
  end
end
```
