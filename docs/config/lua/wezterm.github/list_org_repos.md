# `wezterm.github.list_org_repos(options)`

{{since('nightly')}}

Returns a list of repositories for an organization.

## Parameters

| Parameter | Type | Description |
|-----------|------|-------------|
| `options` | table | Required settings |

## Options

| Option | Type | Default | Description |
|--------|------|---------|-------------|
| `org` | string | required | Organization name |
| `token` | string | nil | GitHub personal access token |
| `per_page` | number | nil | Results per page (max 100) |
| `page` | number | nil | Page number |

## Return Value

Returns an array of repository tables (see [get_repo](get_repo.md) for fields),
or a table with an `error` field on failure.

## Example

```lua
local wezterm = require 'wezterm'

local repos = wezterm.github.list_org_repos { org = 'rust-lang' }

if repos.error then
  wezterm.log_error('Failed: ' .. repos.error)
else
  for _, repo in ipairs(repos) do
    wezterm.log_info(repo.name .. ' - ' .. (repo.description or ''))
  end
end
```
