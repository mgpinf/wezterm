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

For example, this assignment opens the tab navigator directly in fuzzy
filtering mode with customized text:

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
    },
  },
}
```
