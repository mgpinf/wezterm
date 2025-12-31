# `wezterm.crypto.hmac_sha256(key, data)`

{{since('nightly')}}

Computes the HMAC-SHA256 of the input data using the provided key and returns
it as a lowercase hexadecimal string.

HMAC (Hash-based Message Authentication Code) can be used to verify both the
data integrity and authenticity of a message.

```lua
local wezterm = require 'wezterm'

local mac =
  wezterm.crypto.hmac_sha256('secret-key', 'message to authenticate')
```

## Example: Creating a signed token

```lua
local wezterm = require 'wezterm'

local function create_signed_token(payload, secret)
  local signature = wezterm.crypto.hmac_sha256(secret, payload)
  return payload .. '.' .. signature
end

local token = create_signed_token('user_id=123', 'my-secret-key')
```
