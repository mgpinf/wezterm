---
title: wezterm.http.post
tags:
 - utility
 - http
---

# `wezterm.http.post(url [, body [, options]])`

Performs an HTTP POST request to the specified URL and returns an `HttpResponse` object.

## Parameters

* `url` - the URL to POST to
* `body` - optional request body as a string
* `options` - optional table with:
  * `headers` - a table of HTTP headers
  * `params` - a table of query parameters (will be URL-encoded and appended to the URL)
  * `timeout` - request timeout in seconds (default: 30)
  * `body` - alternative way to specify the request body

## HttpResponse

The returned `HttpResponse` object has the following fields:

* `status` - the HTTP status code (e.g., 200, 201)
* `body` - the response body as a string
* `headers` - a table of response headers

## Examples

### POST JSON data

```lua
local wezterm = require 'wezterm'

local payload = wezterm.json_encode {
  name = 'test',
  value = 42,
}

local response = wezterm.http.post('https://api.example.com/items', payload, {
  headers = {
    ['Content-Type'] = 'application/json',
  },
})

if response.status == 201 then
  wezterm.log_info 'Created successfully!'
end
```

### POST form data

```lua
local wezterm = require 'wezterm'

local response = wezterm.http.post(
  'https://api.example.com/login',
  'username=admin&password=secret',
  {
    headers = {
      ['Content-Type'] = 'application/x-www-form-urlencoded',
    },
  }
)
```
