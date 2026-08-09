---
tags:
  - transient
---

# `TransientOption`

{{since('nightly')}}

The `TransientOption` struct specifies information about a command line
flag that can be toggled and requires a value when activated.

It is a lua object with the following fields:
* `key` - text to enter in order to set option
* `default` - optional argument indicating default value.
  If omitted, option is not set
* `description` - text to describe the option
* `flag` - text used as the key in the [TransientResult](./TransientResult.md)
  table
* `allow_nil` - optional argument that determines whether to allow
  setting the option to `nil` if previously set to a string.
  If omitted, option is set to false if previously set.
  Else, user is prompted for value
* `choices` - Optional argument indicating the list of choices
  to select from. If provided, a selector is displayed
  when setting an option. Else, a line prompt is displayed
  If omitted, when setting an option, a line prompt is displayed
  when setting an option
* `metadata` - a table with key value pairs where keys are strings
  and values can be of any data type


Example of `TransientOption` object:

```lua
local option = {
  key = '-t',
  default = '100',
  description = 'Tail',
  flag = '--tail=',
  allow_nil = true,
  choices = { 'choice1', 'choice2' },
}

-- With metadata
local option_with_metadata = {
  key = '-m',
  default = '10',
  description = 'Max count',
  flag = '--max-count=',
  metadata = {
    additional_description = 'Limit the number of matching lines per file searched',
  },
}
```
