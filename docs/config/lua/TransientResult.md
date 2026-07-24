---
tags:
  - transient
---

# `TransientResult`

{{since('nightly')}}

The `TransientResult` is a lua object with the following fields:
* `entries` - a table mapping flag names to their values. Each key is
  the `flag` string from one of the below entries:
  * [TransientSwitch](./TransientSwitch.md)
  * [TransientOption](./TransientOption.md)
  * [TransientCyclicSwitch](./TransientCyclicSwitch.md)

Each value against the key is a lua object with following fields:
- `value` - the value selected against the above flag  
  * For [TransientSwitch](./TransientSwitch.md) — value is `true` or `false`
  * For [TransientOption](./TransientOption.md) — value is the selected string, or `nil`
  * For [TransientCyclicSwitch](./TransientCyclicSwitch.md) — value is the selected choice string, or `nil`
* `metadata` - a table with key value pairs where keys are strings
  and values can be of any data type. This is only present if it was
  passed to the corresponding object which is one of [TransientSwitch](./TransientSwitch.md), 
  [TransientOption](./TransientOption.md) or [TransientCyclicSwitch](./TransientCyclicSwitch.md)

Example of `TransientResult` object:
```lua
local result = {
  entries = {
    ['--follow'] = { value = true },
    ['--tail='] = {
      value = '100',
      metadata = { additional_description = 'Output the last N lines' },
    },
  },
}
```

Example of using `TransientResult`:
```lua
wezterm.action_callback(function(window, pane, result)
  local follow = result.entries['--follow']
  local tail = result.entries['--tail=']
end)
```
