# `wezterm.github.search_repos(options)`

{{since('nightly')}}

Searches for repositories.

## Parameters

| Parameter | Type | Description |
|-----------|------|-------------|
| `options` | table | Required settings |

## Options

| Option | Type | Default | Description |
|--------|------|---------|-------------|
| `query` | string | required | Search query (GitHub search syntax) |
| `token` | string | nil | GitHub personal access token |
| `per_page` | number | nil | Results per page (max 100) |
| `page` | number | nil | Page number |

## Return Value

Returns a table with the following fields, or a table with an `error` field on failure:

| Field | Type | Description |
|-------|------|-------------|
| `total_count` | number | Total number of matching repositories |
| `items` | table | Array of repository tables (see [get_repo](get_repo.md)) |

## Example

```lua
local wezterm = require 'wezterm'

local results = wezterm.github.search_repos {
  query = 'terminal emulator language:rust stars:>1000',
  per_page = 10,
}

if results.error then
  wezterm.log_error('Failed: ' .. results.error)
else
  wezterm.log_info('Found ' .. results.total_count .. ' repositories')
  for _, repo in ipairs(results.items) do
    wezterm.log_info(
      repo.full_name .. ' - ' .. repo.stargazers_count .. ' stars'
    )
  end
end
```
