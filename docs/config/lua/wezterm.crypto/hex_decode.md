# `wezterm.crypto.hex_decode(data)`

{{since('nightly')}}

Decodes a hexadecimal-encoded string and returns the original data as a UTF-8
string.

Returns an error if the input is not valid hexadecimal or if the decoded bytes
are not valid UTF-8.

```lua
local wezterm = require 'wezterm'

local decoded = wezterm.crypto.hex_decode '48656c6c6f'
-- Returns: "Hello"
```

See also: [hex_encode](hex_encode.md)
