# `wezterm.password` module

{{since('nightly')}}

The `wezterm.password` module provides functions for securely storing and
retrieving passwords using the operating system's native credential store:

* **macOS**: Keychain
* **Windows**: Credential Manager
* **Linux**: Secret Service API (via libsecret)

This is useful for storing sensitive data like SSH passphrases, API tokens,
or any credentials that your Lua configuration needs to access without
hardcoding them in plain text.

## Available functions

  - [delete](delete.md) - Delete a stored password
  - [get](get.md) - Retrieve a stored password
  - [set](set.md) - Store a password

## Example: SSH passphrase helper

```lua
local wezterm = require 'wezterm'

-- Store a passphrase (run once, or via a key binding)
-- wezterm.password.set('wezterm-ssh', 'my-key', 'my-passphrase')

-- Later, retrieve it for use
wezterm.on('mux-startup', function()
  local passphrase = wezterm.password.get('wezterm-ssh', 'my-key')
  if passphrase then
    -- Use passphrase for SSH connections
  end
end)
```
