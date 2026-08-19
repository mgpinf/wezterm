---
tags:
  - transient
---

# `TransientResult`

{{since('nightly')}}

The `TransientResult` is a lua table mapping argument names to their values. Each
key is the `argument` string from one of the following entries:

* [TransientSwitch](./TransientSwitch.md) - value is `true` or `false`
* [TransientOption](./TransientOption.md) - value is the selected string, or `nil`

If multiple entries use the same `argument`, the value from the last matching
entry in menu order is returned.

Example of `TransientResult` object:
```lua
local result = {
  ['--follow'] = true,
  ['--tail='] = '100',
}
```

Example of using `TransientResult`:
```lua
wezterm.action_callback(function(window, pane, result)
  local follow = result['--follow']
  local tail = result['--tail=']
end)
```
