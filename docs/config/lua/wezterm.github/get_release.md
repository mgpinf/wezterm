# `wezterm.github.get_release(options)`

{{since('nightly')}}

Returns information about a release.

## Parameters

| Parameter | Type | Description |
|-----------|------|-------------|
| `options` | table | Required settings |

## Options

| Option | Type | Default | Description |
|--------|------|---------|-------------|
| `owner` | string | required | Repository owner |
| `repo` | string | required | Repository name |
| `release_id` | number | required | Release ID |
| `token` | string | nil | GitHub personal access token |

## Return Value

Returns a table with the following fields, or a table with an `error` field on failure:

| Field | Type | Description |
|-------|------|-------------|
| `id` | number | Release ID |
| `tag_name` | string | Git tag name |
| `name` | string | Release name |
| `body` | string | Release description |
| `draft` | bool | Whether it's a draft |
| `prerelease` | bool | Whether it's a prerelease |
| `html_url` | string | Release URL |
| `tarball_url` | string | Tarball download URL |
| `zipball_url` | string | Zipball download URL |
| `created_at` | string | Creation timestamp |
| `published_at` | string | Publication timestamp |
| `assets` | table | Array of asset tables |

### Asset Fields

| Field | Type | Description |
|-------|------|-------------|
| `id` | number | Asset ID |
| `name` | string | File name |
| `size` | number | File size in bytes |
| `download_count` | number | Number of downloads |
| `browser_download_url` | string | Download URL |

## Example

```lua
local wezterm = require 'wezterm'

local release = wezterm.github.get_release {
  owner = 'wez',
  repo = 'wezterm',
  release_id = 12345,
}

if release.error then
  wezterm.log_error('Failed: ' .. release.error)
else
  wezterm.log_info(release.name .. ' (' .. release.tag_name .. ')')
  for _, asset in ipairs(release.assets) do
    wezterm.log_info(
      '  ' .. asset.name .. ' - ' .. asset.download_count .. ' downloads'
    )
  end
end
```
