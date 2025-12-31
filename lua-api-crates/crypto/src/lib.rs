use config::lua::get_or_create_sub_module;
use config::lua::mlua::{self, Lua};
use hmac::{Hmac, Mac};
use sha2::{Digest, Sha256, Sha512};

type HmacSha256 = Hmac<Sha256>;

pub fn register(lua: &Lua) -> anyhow::Result<()> {
    let crypto_mod = get_or_create_sub_module(lua, "crypto")?;

    crypto_mod.set("sha256", lua.create_function(sha256_hash)?)?;
    crypto_mod.set("sha512", lua.create_function(sha512_hash)?)?;
    crypto_mod.set("sha1", lua.create_function(sha1_hash)?)?;
    crypto_mod.set("md5", lua.create_function(md5_hash)?)?;
    crypto_mod.set("hmac_sha256", lua.create_function(hmac_sha256)?)?;
    crypto_mod.set("uuid", lua.create_function(generate_uuid)?)?;
    crypto_mod.set("random_bytes", lua.create_function(random_bytes)?)?;
    crypto_mod.set("base64_encode", lua.create_function(base64_encode)?)?;
    crypto_mod.set("base64_decode", lua.create_function(base64_decode)?)?;
    crypto_mod.set("hex_encode", lua.create_function(hex_encode)?)?;
    crypto_mod.set("hex_decode", lua.create_function(hex_decode)?)?;

    Ok(())
}

fn sha256_hash(_: &Lua, data: mlua::String) -> mlua::Result<String> {
    let mut hasher = Sha256::new();
    hasher.update(data.as_bytes());
    let result = hasher.finalize();
    Ok(hex::encode(result))
}

fn sha512_hash(_: &Lua, data: mlua::String) -> mlua::Result<String> {
    let mut hasher = Sha512::new();
    hasher.update(data.as_bytes());
    let result = hasher.finalize();
    Ok(hex::encode(result))
}

fn sha1_hash(_: &Lua, data: mlua::String) -> mlua::Result<String> {
    use sha1::Sha1;
    let mut hasher = Sha1::new();
    hasher.update(data.as_bytes());
    let result = hasher.finalize();
    Ok(hex::encode(result))
}

fn md5_hash(_: &Lua, data: mlua::String) -> mlua::Result<String> {
    use md5::Md5;
    let mut hasher = Md5::new();
    hasher.update(data.as_bytes());
    let result = hasher.finalize();
    Ok(hex::encode(result))
}

fn hmac_sha256(_: &Lua, (key, data): (mlua::String, mlua::String)) -> mlua::Result<String> {
    let mut mac = HmacSha256::new_from_slice(key.as_bytes())
        .map_err(|e| mlua::Error::external(format!("Invalid key length: {}", e)))?;
    mac.update(data.as_bytes());
    let result = mac.finalize();
    Ok(hex::encode(result.into_bytes()))
}

fn generate_uuid(_: &Lua, _: ()) -> mlua::Result<String> {
    Ok(uuid::Uuid::new_v4().to_string())
}

fn random_bytes(_: &Lua, count: usize) -> mlua::Result<String> {
    if count > 1024 * 1024 {
        return Err(mlua::Error::external(
            "random_bytes: count must not exceed 1MB (1048576 bytes)",
        ));
    }
    let mut bytes = vec![0u8; count];
    getrandom::fill(&mut bytes)
        .map_err(|e| mlua::Error::external(format!("Failed to generate random bytes: {}", e)))?;
    Ok(hex::encode(bytes))
}

fn base64_encode(_: &Lua, data: mlua::String) -> mlua::Result<String> {
    use base64::Engine;
    Ok(base64::engine::general_purpose::STANDARD.encode(data.as_bytes()))
}

fn base64_decode(_: &Lua, data: String) -> mlua::Result<String> {
    use base64::Engine;
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(&data)
        .map_err(|e| mlua::Error::external(format!("Invalid base64: {}", e)))?;
    String::from_utf8(bytes)
        .map_err(|e| mlua::Error::external(format!("Decoded bytes are not valid UTF-8: {}", e)))
}

fn hex_encode(_: &Lua, data: mlua::String) -> mlua::Result<String> {
    Ok(hex::encode(data.as_bytes()))
}

fn hex_decode(_: &Lua, data: String) -> mlua::Result<String> {
    let bytes =
        hex::decode(&data).map_err(|e| mlua::Error::external(format!("Invalid hex: {}", e)))?;
    String::from_utf8(bytes)
        .map_err(|e| mlua::Error::external(format!("Decoded bytes are not valid UTF-8: {}", e)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sha256() {
        let lua = Lua::new();
        let result = sha256_hash(&lua, lua.create_string("hello").unwrap()).unwrap();
        assert_eq!(
            result,
            "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824"
        );
    }

    #[test]
    fn test_sha512() {
        let lua = Lua::new();
        let result = sha512_hash(&lua, lua.create_string("hello").unwrap()).unwrap();
        assert_eq!(
            result,
            "9b71d224bd62f3785d96d46ad3ea3d73319bfbc2890caadae2dff72519673ca72323c3d99ba5c11d7c7acc6e14b8c5da0c4663475c2e5c3adef46f73bcdec043"
        );
    }

    #[test]
    fn test_sha1() {
        let lua = Lua::new();
        let result = sha1_hash(&lua, lua.create_string("hello").unwrap()).unwrap();
        assert_eq!(result, "aaf4c61ddcc5e8a2dabede0f3b482cd9aea9434d");
    }

    #[test]
    fn test_md5() {
        let lua = Lua::new();
        let result = md5_hash(&lua, lua.create_string("hello").unwrap()).unwrap();
        assert_eq!(result, "5d41402abc4b2a76b9719d911017c592");
    }

    #[test]
    fn test_hmac_sha256() {
        let lua = Lua::new();
        let key = lua.create_string("secret").unwrap();
        let data = lua.create_string("hello").unwrap();
        let result = hmac_sha256(&lua, (key, data)).unwrap();
        assert_eq!(
            result,
            "88aab3ede8d3adf94d26ab90d3bafd4a2083070c3bcce9c014ee04a443847c0b"
        );
    }

    #[test]
    fn test_uuid() {
        let lua = Lua::new();
        let result = generate_uuid(&lua, ()).unwrap();
        assert_eq!(result.len(), 36);
        assert_eq!(result.chars().nth(14), Some('4'));
    }

    #[test]
    fn test_random_bytes() {
        let lua = Lua::new();
        let result = random_bytes(&lua, 16).unwrap();
        assert_eq!(result.len(), 32);
    }

    #[test]
    fn test_base64_roundtrip() {
        let lua = Lua::new();
        let original = "Hello, World!";
        let encoded = base64_encode(&lua, lua.create_string(original).unwrap()).unwrap();
        assert_eq!(encoded, "SGVsbG8sIFdvcmxkIQ==");
        let decoded = base64_decode(&lua, encoded).unwrap();
        assert_eq!(decoded, original);
    }

    #[test]
    fn test_hex_roundtrip() {
        let lua = Lua::new();
        let original = "Hello";
        let encoded = hex_encode(&lua, lua.create_string(original).unwrap()).unwrap();
        assert_eq!(encoded, "48656c6c6f");
        let decoded = hex_decode(&lua, encoded).unwrap();
        assert_eq!(decoded, original);
    }
}
