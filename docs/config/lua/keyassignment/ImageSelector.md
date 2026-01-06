---
tags:
  - prompt
  - selector
  - image
---

# `ImageSelector`

Activates an overlay to display a list of images for the user to select from,
with a live preview pane showing the currently highlighted image.

When the user selects an image, emits an event that allows you to act
upon the selection.

`ImageSelector` accepts the following fields:

* `action` - an event callback registered via `wezterm.action_callback`. The
  callback's function signature is `(window, pane, label, path)` where `window` and
  `pane` are the [Window](../window/index.md) and [Pane](../pane/index.md)
  objects from the current pane and window, `label` is the display label (derived
  from filename if not specified), and `path` is the full path to the selected image.
  Both `label` and `path` will be `nil` if the overlay is cancelled without selecting anything.
* `title` - the title that will be set for the overlay pane. Defaults to empty string.
* `choices` - a lua table consisting of the image choices. Each entry is a table with:
  * `path` - (required) the full path to the image file
  * `label` - (optional) display label for the image. If not specified, the filename
    is extracted from the path and used as the label.
* `description` - (required) a string to display in the overlay header.
* `fuzzy` - a boolean that defaults to `false`. If `true`, ImageSelector will start
  in its fuzzy finding mode.
* `fuzzy_description` - a string to display when in fuzzy finding mode. If not
  specified, falls back to the value of `description`.

### Palette Colors

You can customize the colors used in the ImageSelector overlay by setting these
palette colors in your configuration:

* `image_selector_description_fg` - foreground color for the description text
* `image_selector_error_fg` - foreground color for error messages (e.g., when an image fails to load)
* `image_selector_separator_fg` - foreground color for the vertical separator between list and preview
* `image_selector_metadata_fg` - foreground color for image metadata (dimensions, file size, format)
* `image_selector_filename_fg` - foreground color for the filename in the preview pane

```lua
config.colors = {
  image_selector_description_fg = '#89b4fa',
  image_selector_error_fg = '#f38ba8',
  image_selector_separator_fg = '#6c7086',
  image_selector_metadata_fg = '#a6adc8',
  image_selector_filename_fg = '#cdd6f4',
}
```

### Key Assignments

The default key assignments in the ImageSelector are as follows:

| Action  |  Key Assignment |
|---------|-----------------|
| Start fuzzy search (if not in filter mode) | <kbd>/</kbd> |
| Toggle fuzzy search | <kbd>Ctrl</kbd> + <kbd>/</kbd> |
| Add to filtering string (if in fuzzy finding mode) | Any character key |
| Remove from filtering string | <kbd>Backspace</kbd> |
| Exit filter mode (if filtering) | <kbd>Enter</kbd> |
| Select currently highlighted image | <kbd>Ctrl</kbd> + <kbd>Enter</kbd> |
| Move Down | <kbd>DownArrow</kbd> |
|           | <kbd>Ctrl</kbd> + <kbd>N</kbd> |
|           | <kbd>Ctrl</kbd> + <kbd>J</kbd> |
|           | <kbd>j</kbd> (if not in filter mode) |
| Move Up   | <kbd>UpArrow</kbd> |
|           | <kbd>Ctrl</kbd> + <kbd>P</kbd> |
|           | <kbd>Ctrl</kbd> + <kbd>K</kbd> |
|           | <kbd>k</kbd> (if not in filter mode) |
| Move to first | <kbd>g</kbd> (if not in filter mode) |
| Move to last  | <kbd>G</kbd> (if not in filter mode) |
| Cancel    | <kbd>q</kbd> (if not in filter mode) |
|           | <kbd>Ctrl</kbd> + <kbd>G</kbd> |
|           | <kbd>Ctrl</kbd> + <kbd>C</kbd> |
|           | <kbd>Escape</kbd> |

## Example of selecting a background image

This example scans a directory for images and lets you pick one to set as the
terminal background:

```lua
local wezterm = require 'wezterm'
local config = wezterm.config_builder()

config.keys = {
  {
    key = 'B',
    mods = 'CTRL|SHIFT',
    action = wezterm.action_callback(function(window, pane)
      local images_dir = wezterm.home_dir .. '/Pictures/Wallpapers'

      -- Use find_files for efficient recursive search
      local files = wezterm.find_files(images_dir, {
        extensions = { 'png', 'jpg', 'jpeg', 'gif', 'webp' },
      })

      local choices = {}
      for _, path in ipairs(files) do
        table.insert(choices, { path = path })
      end

      if #choices == 0 then
        wezterm.log_warn('No images found in ' .. images_dir)
        return
      end

      window:perform_action(
        wezterm.action.ImageSelector {
          action = wezterm.action_callback(
            function(window, pane, label, path)
              if path then
                window:set_config_overrides {
                  window_background_image = path,
                  window_background_image_hsb = {
                    brightness = 0.1,
                    saturation = 0.5,
                  },
                }
              end
            end
          ),
          title = 'Select Background',
          description = 'Choose a wallpaper',
          fuzzy_description = 'Search wallpapers',
          choices = choices,
        },
        pane
      )
    end),
  },
}

return config
```

## Example with custom labels

You can provide custom labels for each image entry:

```lua
local wezterm = require 'wezterm'
local config = wezterm.config_builder()

config.keys = {
  {
    key = 'T',
    mods = 'CTRL|SHIFT',
    action = wezterm.action.ImageSelector {
      action = wezterm.action_callback(function(window, pane, label, path)
        if path then
          wezterm.log_info('Selected theme: ' .. label .. ' at ' .. path)
        end
      end),
      title = 'Theme Selector',
      description = 'Select a theme',
      choices = {
        { label = 'Dark Forest', path = '/path/to/themes/dark-forest.png' },
        { label = 'Ocean Blue', path = '/path/to/themes/ocean-blue.png' },
        { label = 'Sunset', path = '/path/to/themes/sunset.jpg' },
      },
    },
  },
}

return config
```

## Supported Image Formats

The ImageSelector supports common image formats including:
- PNG
- JPEG/JPG
- GIF
- WebP
- BMP

If an image fails to load, the preview pane will display an appropriate error message:
- `File not found` - the image file doesn't exist
- `Permission denied` - insufficient permissions to read the file
- `Invalid or unsupported image format` - the file is corrupted or not a supported format
- `I/O error: ...` - other file system errors
