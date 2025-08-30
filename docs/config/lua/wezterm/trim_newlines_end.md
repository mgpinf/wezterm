---
title: wezterm.trim_newlines_end
tags:
 - utility
 - string
---
# `wezterm.trim_newlines_end(str)`

{{since('nightly')}}

Returns a string with trailing newline characters
(both `\n` and `\r\n` are recognized as newlines) removed.

```lua
local wezterm = require 'wezterm'

local line = wezterm.trim_newlines_end 'output\n'
wezterm.log_error(line)
```


