# `wezterm.crypto` module

{{since('nightly')}}

The `wezterm.crypto` module provides cryptographic utilities for hashing,
encoding, and generating random data.

## Available functions

### Hash functions

  - [sha256](sha256.md) - SHA-256 hash
  - [sha512](sha512.md) - SHA-512 hash
  - [sha1](sha1.md) - SHA-1 hash
  - [md5](md5.md) - MD5 hash
  - [hmac_sha256](hmac_sha256.md) - HMAC-SHA256

### Random generation

  - [uuid](uuid.md) - Generate UUID v4
  - [random_bytes](random_bytes.md) - Generate random bytes

### Encoding

  - [base64_encode](base64_encode.md) - Base64 encode
  - [base64_decode](base64_decode.md) - Base64 decode
  - [hex_encode](hex_encode.md) - Hexadecimal encode
  - [hex_decode](hex_decode.md) - Hexadecimal decode
