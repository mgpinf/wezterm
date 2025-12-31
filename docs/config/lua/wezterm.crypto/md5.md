# `wezterm.crypto.md5(data)`

{{since('nightly')}}

Computes the MD5 hash of the input data and returns it as a lowercase
hexadecimal string.

!!! warning
    MD5 is considered cryptographically broken and should not be used for
    security purposes. Use SHA-256 or SHA-512 instead.

```lua
local wezterm = require 'wezterm'

local hash = wezterm.crypto.md5 'hello'
-- Returns: "5d41402abc4b2a76b9719d911017c592"
```
