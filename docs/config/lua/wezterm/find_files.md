---
title: wezterm.find_files
tags:
 - utility
 - filesystem
---

# `wezterm.find_files(directory [, options])`

This function recursively searches for files in the specified `directory` and
returns an array containing the absolute file paths of the matching results.
It provides fd-like functionality for finding files with optional filtering
by extension and depth.

The optional `options` parameter is a table that can contain:

* `extensions` - an array of file extensions to filter by (e.g., `{"png", "jpg"}`).
  Extensions can be specified with or without the leading dot. If not specified
  or empty, all files are returned.
* `exclude` - an array of glob patterns to exclude (e.g., `{"*.log", "**/node_modules/**"}`).
  Any file or directory matching these patterns will be skipped.
* `max_depth` - maximum directory depth to traverse (1-based, like `fd`). A value
  of `1` means only the starting directory, `2` includes one level of subdirectories,
  and so on. If not specified, there is no depth limit.
* `hidden` - boolean, whether to include hidden files and directories (those
  starting with `.`). Defaults to `false`.

## Examples

### Find all images in a directory

```lua
local wezterm = require 'wezterm'

local images = wezterm.find_files(wezterm.home_dir .. '/Pictures', {
  extensions = { 'png', 'jpg', 'jpeg', 'gif', 'webp' },
})

for _, path in ipairs(images) do
  wezterm.log_info('Found image: ' .. path)
end
```

### Find all files with depth limit

```lua
local wezterm = require 'wezterm'

-- Only search 2 levels deep
local files = wezterm.find_files('/etc', {
  max_depth = 2,
})
```

### Find all files including hidden

```lua
local wezterm = require 'wezterm'

local all_files = wezterm.find_files(wezterm.home_dir .. '/projects', {
  extensions = { 'rs' },
  hidden = true,
})
```

### Find files with exclusions

```lua
local wezterm = require 'wezterm'

-- Find all Rust files, excluding target directories and test files
local source_files = wezterm.find_files(wezterm.home_dir .. '/projects', {
  extensions = { 'rs' },
  exclude = { '**/target/**', '*_test.rs', '**/.git/**' },
})
```

### Use with ImageSelector

```lua
local wezterm = require 'wezterm'
local config = wezterm.config_builder()

config.keys = {
  {
    key = 'B',
    mods = 'CTRL|SHIFT',
    action = wezterm.action_callback(function(window, pane)
      local images = wezterm.find_files(wezterm.home_dir .. '/Pictures', {
        extensions = { 'png', 'jpg', 'jpeg', 'gif', 'webp' },
      })

      local choices = {}
      for _, path in ipairs(images) do
        table.insert(choices, { path = path })
      end

      if #choices == 0 then
        return
      end

      window:perform_action(
        wezterm.action.ImageSelector {
          action = wezterm.action_callback(
            function(window, pane, label, path)
              if path then
                window:set_config_overrides {
                  window_background_image = path,
                }
              end
            end
          ),
          title = 'Select Background',
          description = 'Choose an image',
          choices = choices,
        },
        pane
      )
    end),
  },
}

return config
```
