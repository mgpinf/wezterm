# `wezterm.github.create_release(options)`

{{since('nightly')}}

Creates a new release.

## Parameters

| Parameter | Type | Description |
|-----------|------|-------------|
| `options` | table | Required settings |

## Options

| Option | Type | Default | Description |
|--------|------|---------|-------------|
| `owner` | string | required | Repository owner |
| `repo` | string | required | Repository name |
| `tag_name` | string | required | Git tag name for the release |
| `token` | string | nil | GitHub personal access token |
| `name` | string | nil | Release name |
| `body` | string | nil | Release description |
| `draft` | bool | false | Create as draft |
| `prerelease` | bool | false | Mark as prerelease |
| `target_commitish` | string | nil | Branch or commit SHA to tag |

## Return Value

Returns the created release table (see [get_release](get_release.md) for fields),
or a table with an `error` field on failure.

## Example

```lua
local wezterm = require 'wezterm'

local token = os.getenv 'GITHUB_TOKEN'
local release = wezterm.github.create_release {
  owner = 'myuser',
  repo = 'myrepo',
  tag_name = 'v1.0.0',
  name = 'Version 1.0.0',
  body = '## Changes\n\n- Initial release',
  token = token,
}

if release.error then
  wezterm.log_error('Failed: ' .. release.error)
else
  wezterm.log_info('Created release: ' .. release.html_url)
end
```
