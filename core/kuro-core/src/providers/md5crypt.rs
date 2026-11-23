//! Unixový crypt(3) s MD5 (`$1$`), tak jak ho používá Webshare při loginu
//! (NSC `providers/md5crypt.ts`).
//!
//! Není to hashování hesla pro naše úložiště — je to formát, který od nás
//! Webshare API očekává: `sha1(md5crypt(heslo, salt))`.
//!
//! Rozdíl proti NSC: heslo se bere jako UTF-8 bajty. NSC ho převádělo
//! kódováním `binary` (latin1), což u hesla s diakritikou dává jiné bajty
//! než PHP/Python implementace. Pro ASCII hesla je výsledek totožný.

use md5::{Digest, Md5};

const ITOA64: &[u8; 64] = b"./0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz";
const MAGIC: &str = "$1$";

fn to64(out: &mut String, mut v: u32, n: usize) {
    for _ in 0..n {
        out.push(ITOA64[(v & 0x3f) as usize] as char);
        v >>= 6;
    }
}

/// Salt může přijít i s prefixem `$1$` a s koncovým `$...`; bere se max. 8 znaků.
fn clean_salt(salt: &str) -> &str {
    let s = salt.strip_prefix(MAGIC).unwrap_or(salt);
    let s = s.split('$').next().unwrap_or("");
    match s.char_indices().nth(8) {
        Some((i, _)) => &s[..i],
        None => s,
    }
}

pub fn md5crypt(password: &str, salt: &str) -> String {
    let salt = clean_salt(salt);
    let pw = password.as_bytes();
    let sb = salt.as_bytes();

    // "Alternativní" součet, jehož bajty se vmíchávají do hlavního.
    let alt = Md5::new().chain_update(pw).chain_update(sb).chain_update(pw).finalize();

    let mut ctx = Md5::new();
    ctx.update(pw);
    ctx.update(MAGIC.as_bytes());
    ctx.update(sb);

    let mut i = pw.len();
    while i > 0 {
        ctx.update(&alt[..i.min(16)]);
        i = i.saturating_sub(16);
    }

    // Bitový trik z původní implementace: podle bitů délky hesla se přidává
    // buď nulový bajt, nebo první bajt hesla.
    let mut i = pw.len();
    while i > 0 {
        if i & 1 == 1 {
            ctx.update([0u8]);
        } else {
            ctx.update(&pw[..1]);
        }
        i >>= 1;
    }

    let mut digest = ctx.finalize();

    for i in 0..1000 {
        let mut c = Md5::new();
        if i & 1 == 1 {
            c.update(pw);
        } else {
            c.update(digest);
        }
        if i % 3 != 0 {
            c.update(sb);
        }
        if i % 7 != 0 {
            c.update(pw);
        }
        if i & 1 == 1 {
            c.update(digest);
        } else {
            c.update(pw);
        }
        digest = c.finalize();
    }

    let d = digest;
    let mut out = String::with_capacity(34);
    out.push_str(MAGIC);
    out.push_str(salt);
    out.push('$');
    for (a, b, c) in [(0, 6, 12), (1, 7, 13), (2, 8, 14), (3, 9, 15), (4, 10, 5)] {
        to64(&mut out, (u32::from(d[a]) << 16) | (u32::from(d[b]) << 8) | u32::from(d[c]), 4);
    }
    to64(&mut out, u32::from(d[11]), 2);
    out
}
