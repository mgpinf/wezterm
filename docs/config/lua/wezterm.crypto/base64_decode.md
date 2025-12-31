# `wezterm.crypto.base64_decode(data)`

{{since('nightly')}}

Decodes a Base64-encoded string and returns the original data as a UTF-8 string.

Returns an error if the input is not valid Base64 or if the decoded bytes are
not valid UTF-8.

```lua
local wezterm = require 'wezterm'

local decoded = wezterm.crypto.base64_decode 'SGVsbG8sIFdvcmxkIQ=='
-- Returns: "Hello, World!"
```

See also: [base64_encode](base64_encode.md)
