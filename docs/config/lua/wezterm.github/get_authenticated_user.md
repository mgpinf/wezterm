# `wezterm.github.get_authenticated_user([options])`

{{since('nightly')}}

Returns information about the authenticated user.

## Parameters

| Parameter | Type | Description |
|-----------|------|-------------|
| `options` | table | Optional settings |

## Options

| Option | Type | Default | Description |
|--------|------|---------|-------------|
| `token` | string | nil | GitHub personal access token |

## Return Value

Returns a table with the following fields, or a table with an `error` field on failure:

| Field | Type | Description |
|-------|------|-------------|
| `login` | string | Username |
| `id` | number | User ID |
| `avatar_url` | string | Avatar URL |
| `html_url` | string | Profile URL |
| `email` | string | Email address (if available) |

## Example

```lua
local wezterm = require 'wezterm'

local token = os.getenv 'GITHUB_TOKEN'
local user = wezterm.github.get_authenticated_user { token = token }

if user.error then
  wezterm.log_error('Failed: ' .. user.error)
else
  wezterm.log_info('Logged in as: ' .. user.login)
end
```
