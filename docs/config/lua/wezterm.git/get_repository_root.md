# `wezterm.git.get_repository_root(path)`

{{since('nightly')}}

Returns the root directory of the git repository containing the specified path,
or `nil` if the path is not inside a git repository.

```lua
local wezterm = require 'wezterm'

local root = wezterm.git.get_repository_root '/path/to/project/src/file.rs'
if root then
  wezterm.log_info('Repository root: ' .. root)
  -- e.g., "/path/to/project"
end
```
