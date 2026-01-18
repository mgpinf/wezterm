---
title: wezterm.password.get
tags:
 - utility
 - password
---

# `wezterm.password.get(service, account)`

{{since('nightly')}}

Retrieves a password from the operating system's credential store.

## Parameters

* `service` - The service name (e.g., application name or URL)
* `account` - The account/username associated with the credential

## Returns

* The password string if found
* `nil` if no matching credential exists

## Errors

Raises an error if the credential store cannot be accessed.

## Example

```lua
local wezterm = require 'wezterm'

local password = wezterm.password.get('my-ssh-host', 'passphrase')
if password then
  -- Use the password
else
  wezterm.log_warn 'No password found'
end
```
