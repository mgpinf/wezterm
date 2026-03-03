---
tags:
  - transient
  - selector
---

# `SelectorActionsResult`

{{since('nightly')}}

The `SelectorActionsResult` struct is a lua object with the following fields:
* `choices` - a lua table consisting of the selected choices. Each entry
  is itself a table with a `label` field, an optional `id` field, and an
  optional `metadata` field. The metadata is a table of string key-value
  pairs that were attached to the original choice.


Example of `SelectorActionsResult` object:

```lua
local result = {
  choices = {
    { label = 'choice1', id = 'random_id', metadata = { status = 'running' } },
    { label = 'choice2' },
  },
}
```
