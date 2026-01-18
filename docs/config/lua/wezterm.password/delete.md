---
title: wezterm.password.delete
tags:
 - utility
 - password
---

# `wezterm.password.delete(service, account)`

{{since('nightly')}}

Deletes a password from the operating system's credential store.

## Parameters

* `service` - The service name (e.g., application name or URL)
* `account` - The account/username associated with the credential

## Returns

* `true` if the credential was deleted
* `false` if no matching credential existed

## Errors

Raises an error if the credential store cannot be accessed.

## Example

```lua
local wezterm = require 'wezterm'

local deleted = wezterm.password.delete('my-ssh-host', 'passphrase')
if deleted then
  wezterm.log_info 'Password deleted'
else
  wezterm.log_info 'No password to delete'
end
```
