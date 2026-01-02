---
tags:
  - transient
---

# `TransientResult`

{{since('nightly')}}

The `TransientResult` struct is a lua object with the following fields:
* `entries` - list of [TransientResultEntry](#transientresultentry) objects


### `TransientResultEntry`

`TransientResultEntry` struct is a lua object with the following fields:
* `flag` - text passed against flag in one of the below entries:
  * [TransientSwitch](./TransientSwitch.md)
  * [TransientOption](./TransientOption.md)
  * [TransientCyclicSwitch](./TransientCyclicSwitch.md)
* `value` - the value selected for entity with above flag
* `tag` - optional string that was set on the entry. This can be used
  to identify entries by a custom identifier rather than by flag


Example of `TransientResult` object:
```lua
local result = {
  entries = {
    { flag = '--follow', value = true, tag = 'follow_logs' },
    { flag = '--tail=', value = '100', tag = 'tail_lines' },
  },
}
```

Example of using `tag` to identify entries:
```lua
wezterm.action_callback(function(window, pane, result)
  for _, entry in ipairs(result.entries) do
    if entry.tag == 'owner' then
      -- use entry.value as owner
    elseif entry.tag == 'repo' then
      -- use entry.value as repo
    end
  end
end)
```
