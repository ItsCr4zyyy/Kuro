//! Vektory z NSC (`openssl passwd -1 -salt <salt> <heslo>`). Když tohle
//! neprojde, login na Webshare nebude fungovat.

use kuro_core::providers::md5crypt::md5crypt;

#[test]
fn openssl_vectors() {
    let vectors = [
        ("password", "$1$salt", "$1$salt$qJH7.N4xYta3aEG/dfqo/0"),
        ("Hello world!", "$1$saltstri", "$1$saltstri$YMyguxXMBpd2TEZ.vS/3q1"),
        ("abcdefghijklmnop", "$1$12345678", "$1$12345678$Sy0u0qLp5W1f/aNE5VcBp/"),
        ("p@ssw0rd!#$%", "$1$aB3dEf9z", "$1$aB3dEf9z$6pYsjn8Xix1MjzIYG7MOd."),
        ("a", "$1$xy", "$1$xy$93sbPXpiY9Sa7GrWxhkre0"),
        (
            "tohle-je-velmi-dlouhe-heslo-pres-tricet-znaku",
            "$1$8charsss",
            "$1$8charsss$qWwP38LT0KbbrwV1z8.FX.",
        ),
    ];
    for (pw, salt, expected) in vectors {
        assert_eq!(md5crypt(pw, salt), expected, "md5crypt({pw:?}, {salt})");
    }
}

#[test]
fn salt_is_truncated_to_8_chars() {
    assert!(md5crypt("password", "$1$saltsaltsalt$ignorovane").starts_with("$1$saltsalt$"));
}

#[test]
fn bare_salt_without_magic() {
    assert_eq!(md5crypt("password", "salt"), "$1$salt$qJH7.N4xYta3aEG/dfqo/0");
}
