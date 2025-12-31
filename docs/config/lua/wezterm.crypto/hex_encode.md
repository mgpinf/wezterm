# `wezterm.crypto.hex_encode(data)`

{{since('nightly')}}

Encodes the input data as a lowercase hexadecimal string.

```lua
local wezterm = require 'wezterm'

local encoded = wezterm.crypto.hex_encode 'Hello'
-- Returns: "48656c6c6f"
```

See also: [hex_decode](hex_decode.md)
