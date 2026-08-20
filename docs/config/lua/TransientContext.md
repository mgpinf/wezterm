---
tags:
  - transient
---

# `TransientContext`

{{since('nightly')}}

The `TransientContext` struct is a lua object with the following fields:
* `header` - text to display as header of the context section
* `entries` - list of [TransientContextEntry](#transientcontextentry) objects


### `TransientContextEntry`

`TransientContextEntry` struct is a lua object with the following fields:
* `label` - key for the entry
* `value` - value for the entry


Example of `TransientContext` object:

```lua
local context = {
  header = 'Context header',
  entries = {
    { label = 'context1', value = 'val1' },
    { label = 'context2', value = 'val2' },
  },
}
```
