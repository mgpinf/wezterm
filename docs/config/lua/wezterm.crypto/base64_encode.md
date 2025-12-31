# `wezterm.crypto.base64_encode(data)`

{{since('nightly')}}

Encodes the input data as a Base64 string.

```lua
local wezterm = require 'wezterm'

local encoded = wezterm.crypto.base64_encode 'Hello, World!'
-- Returns: "SGVsbG8sIFdvcmxkIQ=="
```

See also: [base64_decode](base64_decode.md)
