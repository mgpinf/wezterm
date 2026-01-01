# `wezterm.git.is_repository(path)`

{{since('nightly')}}

Returns `true` if the specified path is inside a git repository, `false` otherwise.

```lua
local wezterm = require 'wezterm'

if wezterm.git.is_repository '/path/to/project' then
  wezterm.log_info 'This is a git repository'
end
```
