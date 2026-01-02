# `wezterm.github.search_code(options)`

{{since('nightly')}}

Searches for code in repositories.

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
| `total_count` | number | Total number of matching files |
| `items` | table | Array of code result tables |

### Code Result Fields

| Field | Type | Description |
|-------|------|-------------|
| `name` | string | File name |
| `path` | string | File path |
| `sha` | string | File SHA |
| `html_url` | string | File URL on GitHub |
| `repo_full_name` | string | Repository full name |

## Example

```lua
local wezterm = require 'wezterm'

local token = os.getenv 'GITHUB_TOKEN'
local results = wezterm.github.search_code {
  query = 'repo:wez/wezterm extension:lua config',
  token = token,
  per_page = 10,
}

if results.error then
  wezterm.log_error('Failed: ' .. results.error)
else
  wezterm.log_info('Found ' .. results.total_count .. ' files')
  for _, item in ipairs(results.items) do
    wezterm.log_info(item.repo_full_name .. '/' .. item.path)
  end
end
```

Note: Code search requires authentication. Without a token, you may receive an error.
