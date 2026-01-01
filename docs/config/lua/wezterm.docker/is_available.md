# `wezterm.docker.is_available()`

{{since('nightly')}}

Returns `true` if Docker is available and running, `false` otherwise.

```lua
local wezterm = require 'wezterm'

if wezterm.docker.is_available() then
  wezterm.log_info 'Docker is running'
else
  wezterm.log_info 'Docker is not available'
end
```
