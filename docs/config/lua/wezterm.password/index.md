---
title: wezterm.password
tags:
 - utility
 - password
---

# `wezterm.password`

{{since('nightly')}}

The `wezterm.password` module provides cross-platform access to the operating
system's credential store:

* **macOS**: Keychain
* **Windows**: Credential Manager
* **Linux/FreeBSD**: Secret Service (GNOME Keyring, KWallet)

This is useful for securely storing and retrieving secrets such as API tokens,
SSH passphrases, or unlock passwords without hardcoding them in your config.

## Functions

| Function | Purpose |
|----------|---------|
| [get(service, account)](get.md) | Retrieve a password |
| [set(service, account, password)](set.md) | Store a password |
| [delete(service, account)](delete.md) | Delete a password |

## Example

```lua
local wezterm = require 'wezterm'

-- Retrieve a stored password
local token = wezterm.password.get('myapp', 'api_token')
if token then
  wezterm.log_info 'Token retrieved successfully'
end

-- Store a new password
wezterm.password.set('myapp', 'api_token', 'secret123')

-- Delete a password
wezterm.password.delete('myapp', 'api_token')
```
