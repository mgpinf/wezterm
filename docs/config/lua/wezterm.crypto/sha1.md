# `wezterm.crypto.sha1(data)`

{{since('nightly')}}

Computes the SHA-1 hash of the input data and returns it as a lowercase
hexadecimal string.

!!! warning
    SHA-1 is considered cryptographically weak and should not be used for
    security purposes. Use SHA-256 or SHA-512 instead.

```lua
local wezterm = require 'wezterm'

local hash = wezterm.crypto.sha1 'hello'
-- Returns: "aaf4c61ddcc5e8a2dabede0f3b482cd9aea9434d"
```
