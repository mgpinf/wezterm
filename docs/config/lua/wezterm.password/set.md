---
title: wezterm.password.set
tags:
 - utility
 - password
---

# `wezterm.password.set(service, account, password)`

{{since('nightly')}}

Stores a password in the operating system's credential store.

If a credential with the same service and account already exists, it will be
updated with the new password.

## Parameters

* `service` - The service name (e.g., application name or URL)
* `account` - The account/username associated with the credential
* `password` - The password string to store

## Returns

* `true` on success

## Errors

Raises an error if the credential store cannot be accessed or the password
cannot be stored.

## Example

```lua
local wezterm = require 'wezterm'

wezterm.password.set('my-ssh-host', 'passphrase', 'supersecret')
```
