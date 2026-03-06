---
tags:
  - transient
---

# `TransientCyclicSwitch`

{{since('nightly')}}

The `TransientCyclicSwitch` struct specifies information about a
command line flag that can be toggled between `nil` and one of
provided values.

It is a lua object with the following fields:
* `key` - text to enter in order to set cyclic switch
* `label` - optional display text shown in the menu instead of `key`.
  Useful for showing a friendlier name (e.g. `"SPC"` instead of `" "`)
* `default` - optional argument indicating default value.
  If omitted, cyclic switch is not set
* `description` - text to describe the cyclic switch
* `flag` - text used as the key in the [TransientResult](./TransientResult.md)
  entries table
* `allow_nil` - Optional argument that determines whether to
  allow to set the cyclic switch to `nil` if currently at the
  last choice. If omitted, cyclic switch is set to `nil` if
  at the last choice. Else cyclic switch is set to first choice.
  Defaults to `true`
* `choices` - the list of choices to toggle between


Example of `TransientCyclicSwitch` object:

```lua
local cyclic_switch = {
  key = '-n',
  default = 'choice1',
  description = 'Sample Cyclic Switch',
  flag = '--sample-flag',
  choices = { 'choice1', 'choice2' },
  allow_nil = true,
}

-- With a custom label
local cyclic_with_label = {
  key = ' ',
  label = 'SPC',
  description = 'Cycle format',
  flag = '--format',
  choices = { 'json', 'yaml', 'toml' },
}
```
