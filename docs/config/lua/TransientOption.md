---
tags:
  - transient
---

# `TransientOption`

{{since('nightly')}}

The `TransientOption` struct specifies a command-line option that takes a
string value.

It is a lua object with the following fields:

* `key` - text to enter in order to set option
* `default` - optional initial value. If omitted, the option is unset
* `description` - text to describe the option
* `argument` - text used as the key in the [TransientResult](./TransientResult.md)
  table
* `allow_unset` - optional boolean that controls whether pressing the entry key
  while the option is set can unset it. Defaults to `true`
* `choices` - optional non-empty list of unique allowed values
* `input` - optional input method:
  * `"prompt"` reads an arbitrary value using a line editor and cannot be used
    with `choices`
  * `"select"` opens a searchable selector and requires `choices`
  * `"cycle"` advances through `choices` each time the entry key is pressed

If `input` is omitted, it defaults to `"prompt"` when `choices` is absent and
to `"select"` when `choices` is present. For `"select"` and `"cycle"`, the
`default` value must be one of the configured choices.

Example of `TransientOption` object:

```lua
local option = {
  key = '-t',
  default = 'choice1',
  description = 'Tail',
  argument = '--tail=',
  allow_unset = true,
  choices = { 'choice1', 'choice2' },
  input = 'select',
}
```

To cycle through the values without opening a selector:

```lua
local option = {
  key = 'o',
  description = 'Order',
  argument = '--order=',
  choices = { 'topological', 'date', 'author-date' },
  input = 'cycle',
}
```
