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
* `label` - optional display text shown in the menu instead of `key`.
  Useful for showing a friendlier name (e.g. `"SPC"` instead of `" "`)
* `default` - optional argument indicating default value.
  If omitted, switch is not set
* `description` - text to describe the switch
* `flag` - text used as the key in the [TransientResult](./TransientResult.md)
  entries table
* `metadata` - a table with key value pairs where keys are strings
  and values can be of any data type


Example of `TransientSwitch` object:

```lua
local switch = {
  key = '-f',
  default = false,
  description = 'Follow',
  flag = '--follow',
}

-- With a custom label
local switch_with_label = {
  key = ' ',
  label = 'SPC',
  default = false,
  description = 'Toggle fullscreen',
  flag = '--fullscreen',
}

-- With metadata
local switch_with_metadata = {
  key = '-l',
  default = false,
  description = 'Line number',
  flag = '--line-number',
  metadata = {
    additional_description = 'Shows line numbers',
  },
}
```
