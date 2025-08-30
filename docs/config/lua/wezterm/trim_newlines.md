---
title: wezterm.trim_newlines
tags:
 - utility
 - string
---
# `wezterm.trim_newlines(str)`

{{since('nightly')}}

Returns a string with leading and trailing newline characters
(both `\n` and `\r\n` are recognized as newlines) removed.

```lua
local wezterm = require 'wezterm'

local line = wezterm.trim_newlines ' output\n'
wezterm.log_error(line)
```


