---
tags:
  - transient
---

# `TransientSwitch`

{{since('nightly')}}

The `TransientSwitch` struct specifies information about a command line
flag that can be toggled and don't require a value when activated.

It is a lua object with the following fields:
* `key` - text to enter in order to toggle switch
* `default` - optional argument indicating default value.
  If omitted, switch is not set
* `description` - text to describe the switch
* `flag` - text used as the key in the [TransientResult](./TransientResult.md)
  entries table


Example of `TransientSwitch` object:

```lua
local switch = {
  key = '-f',
  default = false,
  description = 'Follow',
  flag = '--follow',
}
```
