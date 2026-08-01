# `ShowTabNavigator`

Activate the tab navigator UI in the current tab.  The tab
navigator displays a list of tabs and allows you to select
and activate a tab from that list.

```lua
config.keys = {
  { key = 'F9', mods = 'ALT', action = wezterm.action.ShowTabNavigator },
}
```

{{since('nightly')}}

The choice corresponding to the current tab is initially selected.

{{since('nightly')}}

`ShowTabNavigator` optionally accepts a lua table with the following fields:

* `title` - the title shown while the tab navigator is active. Defaults to
  `"Tab Navigator"`.
* `help_text` - the text displayed while the tab navigator is in its default
  selection mode. Defaults to
  `"Select an item and press Enter=launch  Esc=cancel  /=filter"`.
* `fuzzy` - when `true`, opens the tab navigator directly in fuzzy filtering
  mode. Defaults to `false`.
* `fuzzy_help_text` - the text displayed before the search term while fuzzy
  filtering is active. Defaults to `"Fuzzy matching: "`.
* `delimiter` - the string displayed after each keyboard selection label and
  after each tab title. Defaults to `"."`. Set this to an empty string to
  omit the delimiter.
* `pane_count_in_suffix` - when `true`, displays the pane count as a separately
  styled suffix rather than as part of the tab label. Pane counts in the suffix
  are not included in fuzzy matching. Defaults to `false`, which preserves the
  previous display and search behavior.
* `dimensions` - optional [OverlayDimensions](../OverlayDimensions.md) controlling
  the size of the tab navigator. Defaults to the full tab.

For example, this assignment opens the tab navigator directly in fuzzy
filtering mode with customized text, uses `":"` as the delimiter, and displays
the pane count as a separate suffix:

```lua
config.keys = {
  {
    key = 'F9',
    mods = 'ALT',
    action = wezterm.action.ShowTabNavigator {
      title = 'Tabs',
      help_text = 'Select a tab',
      fuzzy = true,
      fuzzy_help_text = 'Search tabs: ',
      delimiter = ':',
      pane_count_in_suffix = true,
    },
  },
}
```
