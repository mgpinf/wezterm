# `wezterm.crypto.uuid()`

{{since('nightly')}}

Generates a random UUID (Universally Unique Identifier) version 4.

Returns the UUID as a string in the standard format:
`xxxxxxxx-xxxx-4xxx-yxxx-xxxxxxxxxxxx`

```lua
local wezterm = require 'wezterm'

local id = wezterm.crypto.uuid()
-- Returns something like: "550e8400-e29b-41d4-a716-446655440000"
```

## Example: Generate unique session identifier

```lua
local wezterm = require 'wezterm'

wezterm.on('gui-startup', function()
  local session_id = wezterm.crypto.uuid()
  wezterm.log_info('Session started: ' .. session_id)
end)
```
