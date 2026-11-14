//! T4.7 `crypto`：摘要 / HMAC / AES-CBC / AES-GCM / DES / 3DES / Base64 / Hex / URL 编码。
//! 算法名与 `requires.crypto` 一一对应；全部 RustCrypto 纯 Rust 实现，三端字节一致。

use crate::error::{Code, Diagnostic};
use aes::cipher::{block_padding::Pkcs7, BlockDecryptMut, BlockEncryptMut, KeyIvInit};
use base64::Engine;
use hmac::Mac;

type Aes128CbcEnc = cbc::Encryptor<aes::Aes128>;
type Aes128CbcDec = cbc::Decryptor<aes::Aes128>;
type Aes256CbcEnc = cbc::Encryptor<aes::Aes256>;
type Aes256CbcDec = cbc::Decryptor<aes::Aes256>;
type DesCbcEnc = cbc::Encryptor<des::Des>;
type DesCbcDec = cbc::Decryptor<des::Des>;
type TdesCbcEnc = cbc::Encryptor<des::TdesEde3>;
type TdesCbcDec = cbc::Decryptor<des::TdesEde3>;

fn b64() -> base64::engine::GeneralPurpose {
    base64::engine::general_purpose::STANDARD
}

/// 摘要族（十六进制小写）。
pub fn digest(alg: &str, data: &[u8]) -> Result<Vec<u8>, Diagnostic> {
    use md5::{Digest, Md5};
    use sha1::Sha1;
    use sha2::{Sha256, Sha512};
    Ok(match alg {
        "md5" => Md5::digest(data).to_vec(),
        "sha1" => Sha1::digest(data).to_vec(),
        "sha256" => Sha256::digest(data).to_vec(),
        "sha512" => Sha512::digest(data).to_vec(),
        _ => {
            return Err(Diagnostic::new(
                Code::EScript,
                "$",
                format!("未知摘要算法 `{alg}`"),
                "md5 | sha1 | sha256 | sha512",
            ))
        }
    })
}

pub fn hmac_hex(alg: &str, key: &[u8], data: &[u8]) -> Result<String, Diagnostic> {
    let mac_err = || {
        Diagnostic::new(Code::EScript, "$", "HMAC key 无效", "key 不能为空")
    };
    Ok(match alg {
        "sha1" | "hmac-sha1" => {
            let mut m = <hmac::Hmac<sha1::Sha1> as Mac>::new_from_slice(key).map_err(|_| mac_err())?;
            m.update(data);
            hex::encode(m.finalize().into_bytes())
        }
        "sha256" | "hmac-sha256" => {
            let mut m = <hmac::Hmac<sha2::Sha256> as Mac>::new_from_slice(key).map_err(|_| mac_err())?;
            m.update(data);
            hex::encode(m.finalize().into_bytes())
        }
        _ => {
            return Err(Diagnostic::new(
                Code::EScript,
                "$",
                format!("未知 HMAC 算法 `{alg}`"),
                "hmac-sha1 | hmac-sha256",
            ))
        }
    })
}

fn len_err(alg: &str, want: &str) -> Diagnostic {
    Diagnostic::new(
        Code::EDecode,
        "$",
        format!("{alg} key 长度应为 {want} 字节，实际 {}", 0),
        "对齐站点密钥长度",
    )
}

fn decode_err(alg: &str, detail: &str) -> Diagnostic {
    Diagnostic::new(
        Code::EDecode,
        "$",
        format!("{alg} 解密失败：{detail}"),
        "确认算法/mode/key/iv 与站点一致；padding 默认 PKCS7",
    )
}

/// 对称加解密。mode: encrypt | decrypt。
pub fn symmetric(
    alg: &str,
    mode: &str,
    key: &[u8],
    iv: &[u8],
    data: &[u8],
    _padding: Option<&str>,
) -> Result<Vec<u8>, Diagnostic> {
    let decrypt = mode == "decrypt";
    match alg {
        "aes-cbc" => {
            if iv.len() != 16 {
                return Err(Diagnostic::new(
                    Code::EDecode,
                    "$",
                    format!("IV 需要 16 字节，实际 {}", iv.len()),
                    "AES-CBC 的 IV 为 16 字节",
                ));
            }
            let dec = |d: &[u8]| -> Result<Vec<u8>, Diagnostic> {
                if d.len() % 16 != 0 {
                    return Err(decode_err(alg, "数据长度不是 16 的倍数"));
                }
                match key.len() {
                    16 => Aes128CbcDec::new(key.into(), iv.into())
                        .decrypt_padded_vec_mut::<Pkcs7>(d)
                        .map_err(|_| decode_err(alg, "padding 校验失败")),
                    32 => Aes256CbcDec::new(key.into(), iv.into())
                        .decrypt_padded_vec_mut::<Pkcs7>(d)
                        .map_err(|_| decode_err(alg, "padding 校验失败")),
                    _ => Err(len_err(alg, "16/32")),
                }
            };
            let enc = |d: &[u8]| -> Result<Vec<u8>, Diagnostic> {
                match key.len() {
                    16 => Ok(Aes128CbcEnc::new(key.into(), iv.into()).encrypt_padded_vec_mut::<Pkcs7>(d)),
                    32 => Ok(Aes256CbcEnc::new(key.into(), iv.into()).encrypt_padded_vec_mut::<Pkcs7>(d)),
                    _ => Err(len_err(alg, "16/32")),
                }
            };
            if decrypt {
                dec(data)
            } else {
                enc(data)
            }
        }
        "aes-gcm" => {
            use aes_gcm::aead::{Aead, KeyInit};
            type Gcm128 = aes_gcm::AesGcm<aes::Aes128, aes_gcm::aead::consts::U12>;
            type Gcm256 = aes_gcm::AesGcm<aes::Aes256, aes_gcm::aead::consts::U12>;
            if iv.len() != 12 {
                return Err(Diagnostic::new(
                    Code::EDecode,
                    "$",
                    format!("GCM nonce 需要 12 字节，实际 {}", iv.len()),
                    "AES-GCM 标准 nonce 为 12 字节",
                ));
            }
            let nonce = aes_gcm::Nonce::from_slice(iv);
            let r: Result<Vec<u8>, Diagnostic> = match key.len() {
                16 => {
                    let c = Gcm128::new_from_slice(key).map_err(|_| len_err(alg, "16/32"))?;
                    if decrypt {
                        c.decrypt(nonce, data).map_err(|_| decode_err(alg, "tag 校验失败"))
                    } else {
                        c.encrypt(nonce, data).map_err(|_| decode_err(alg, "加密失败"))
                    }
                }
                32 => {
                    let c = Gcm256::new_from_slice(key).map_err(|_| len_err(alg, "16/32"))?;
                    if decrypt {
                        c.decrypt(nonce, data).map_err(|_| decode_err(alg, "tag 校验失败"))
                    } else {
                        c.encrypt(nonce, data).map_err(|_| decode_err(alg, "加密失败"))
                    }
                }
                _ => Err(len_err(alg, "16/32")),
            };
            r
        }
        "des" => {
            if key.len() != 8 {
                return Err(len_err(alg, "8"));
            }
            if iv.len() != 8 {
                return Err(Diagnostic::new(Code::EDecode, "$", format!("DES IV 需要 8 字节，实际 {}", iv.len()), "DES-CBC 的 IV 为 8 字节"));
            }
            if decrypt {
                if data.len() % 8 != 0 {
                    return Err(decode_err(alg, "数据长度不是 8 的倍数"));
                }
                DesCbcDec::new(key.into(), iv.into())
                    .decrypt_padded_vec_mut::<Pkcs7>(data)
                    .map_err(|_| decode_err(alg, "padding 校验失败"))
            } else {
                Ok(DesCbcEnc::new(key.into(), iv.into()).encrypt_padded_vec_mut::<Pkcs7>(data))
            }
        }
        "triple-des" | "3des" => {
            if key.len() != 24 {
                return Err(len_err(alg, "24"));
            }
            if iv.len() != 8 {
                return Err(Diagnostic::new(Code::EDecode, "$", format!("3DES IV 需要 8 字节，实际 {}", iv.len()), "3DES-CBC 的 IV 为 8 字节"));
            }
            if decrypt {
                if data.len() % 8 != 0 {
                    return Err(decode_err(alg, "数据长度不是 8 的倍数"));
                }
                TdesCbcDec::new(key.into(), iv.into())
                    .decrypt_padded_vec_mut::<Pkcs7>(data)
                    .map_err(|_| decode_err(alg, "padding 校验失败"))
            } else {
                Ok(TdesCbcEnc::new(key.into(), iv.into()).encrypt_padded_vec_mut::<Pkcs7>(data))
            }
        }
        _ => Err(Diagnostic::new(
            Code::EScript,
            "$",
            format!("未知对称算法 `{alg}`"),
            "aes-cbc | aes-gcm | des | triple-des",
        )),
    }
}

pub fn base64_encode(data: &[u8]) -> String {
    b64().encode(data)
}

/// SHA-256 → 小写 hex（build manifest / x-jslib 校验用，E1/E2）。
pub fn sha256_hex(data: &[u8]) -> String {
    use sha2::Digest;
    hex::encode(sha2::Sha256::digest(data))
}

pub fn base64_decode(s: &str) -> Result<Vec<u8>, Diagnostic> {
    b64().decode(s.trim()).map_err(|e| {
        Diagnostic::new(
            Code::EScript,
            "$",
            format!("Base64 解码失败：{e}"),
            "确认输入是标准 Base64（含填充）",
        )
    })
}

/// Base64 → 小写 hex（脚本桥用：二进制以 hex 安全字符串穿越 JS 边界）。
pub fn base64_to_hex(s: &str) -> Result<String, Diagnostic> {
    Ok(hex::encode(base64_decode(s)?))
}

/// Base64 → UTF-8 字符串（解码结果按文本使用）。
pub fn base64_to_utf8(s: &str) -> Result<String, Diagnostic> {
    let bytes = base64_decode(s)?;
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

pub fn url_encode(s: &str, charset: Option<&str>) -> String {
    match charset {
        Some("gbk") | Some("gb2312") | Some("gb18030") => {
            let (enc, _, _) = encoding_rs::GBK.encode(s);
            percent(&enc)
        }
        Some("big5") => {
            let (enc, _, _) = encoding_rs::BIG5.encode(s);
            percent(&enc)
        }
        _ => percent(s.as_bytes()),
    }
}

fn percent(data: &[u8]) -> String {
    let mut out = String::new();
    for b in data {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(*b as char)
            }
            _ => out.push_str(&format!("%{:02X}", b)),
        }
    }
    out
}

pub fn url_decode(s: &str, charset: Option<&str>) -> Result<String, Diagnostic> {
    let bytes = percent_decode(s)?;
    let (r, _, _) = match charset {
        Some("gbk") | Some("gb2312") => encoding_rs::GBK.decode(&bytes),
        Some("big5") => encoding_rs::BIG5.decode(&bytes),
        _ => encoding_rs::UTF_8.decode(&bytes),
    };
    Ok(r.into_owned())
}

pub fn percent_decode(s: &str) -> Result<Vec<u8>, Diagnostic> {
    let b: Vec<u8> = s.replace('+', " ").bytes().collect();
    let mut out = vec![];
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() {
            let hex = std::str::from_utf8(&b[i + 1..i + 3]).unwrap_or("zz");
            if let Ok(v) = u8::from_str_radix(hex, 16) {
                out.push(v);
                i += 3;
                continue;
            }
        }
        out.push(b[i]);
        i += 1;
    }
    Ok(out)
}
