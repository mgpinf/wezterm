# `wezterm.github.get_gist(options)`

{{since('nightly')}}

Returns information about a gist.

## Parameters

| Parameter | Type | Description |
|-----------|------|-------------|
| `options` | table | Required settings |

## Options

| Option | Type | Default | Description |
|--------|------|---------|-------------|
| `id` | string | required | Gist ID |
| `token` | string | nil | GitHub personal access token |

## Return Value

Returns a table with the following fields, or a table with an `error` field on failure:

| Field | Type | Description |
|-------|------|-------------|
| `id` | string | Gist ID |
| `description` | string | Gist description |
| `public` | bool | Whether the gist is public |
| `html_url` | string | Gist URL |
| `comments` | number | Number of comments |
| `created_at` | string | Creation timestamp |
| `updated_at` | string | Last update timestamp |
| `files` | table | Map of filename to file info |

### File Fields

| Field | Type | Description |
|-------|------|-------------|
| `filename` | string | File name |
| `language` | string | Detected language |
| `size` | number | File size in bytes |
| `content` | string | File content |

## Example

```lua
local wezterm = require 'wezterm'

local gist = wezterm.github.get_gist { id = 'abc123def456' }

if gist.error then
  wezterm.log_error('Failed: ' .. gist.error)
else
  wezterm.log_info(gist.description)
  for filename, file in pairs(gist.files) do
    wezterm.log_info(filename .. ' (' .. file.language .. ')')
  end
end
```
