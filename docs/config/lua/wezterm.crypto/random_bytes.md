# `wezterm.crypto.random_bytes(count)`

{{since('nightly')}}

Generates cryptographically secure random bytes and returns them as a lowercase
hexadecimal string.

The `count` parameter specifies the number of random bytes to generate. The
returned string will be twice as long (2 hex characters per byte).

The maximum allowed count is 1MB (1,048,576 bytes).

```lua
local wezterm = require 'wezterm'

-- Generate 16 random bytes (returns 32 hex characters)
local random_hex = wezterm.crypto.random_bytes(16)
-- Returns something like: "a1b2c3d4e5f6a7b8c9d0e1f2a3b4c5d6"
```

## Example: Generate a random token

```lua
local wezterm = require 'wezterm'

-- Generate a 32-byte (256-bit) random token
local token = wezterm.crypto.random_bytes(32)
```
