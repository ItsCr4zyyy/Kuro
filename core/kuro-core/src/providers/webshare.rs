//! Webshare.cz (NSC `providers/webshare.ts`).
//!
//! Webshare vrací XML, ne JSON, a chyby signalizuje v `<status>`, ne HTTP
//! kódem — HTTP 200 se `<status>FATAL</status>` je běžný stav.
//!
//! Zkušenosti z NSC, které tu platí:
//! - souběh nejvýš 3 dotazy, při víc začne Webshare omezovat;
//! - pod zátěží hlásí `File not found` i u živých souborů, takže jedno
//!   selhání neznamená mrtvý soubor;
//! - odkaz z `file_link` je vázaný na účet a IP a vyprší (~hodiny).

use std::sync::RwLock;

use async_trait::async_trait;
use md5::{Digest, Md5};
use roxmltree::{Document, Node};
use sha1::Sha1;

use super::md5crypt::md5crypt;
use super::{Account, LinkPurpose, Provider, ProviderId, RawFile, ResolvedLink, SearchOptions, SearchSort};
use crate::error::{Error, Result};

const BASE: &str = "https://webshare.cz/api";

/// Konzervativní odhad platnosti odkazu; přesnou expiraci Webshare nehlásí.
const LINK_TTL_MS: i64 = 4 * 60 * 60 * 1000;

pub struct Webshare {
    http: reqwest::Client,
    base: String,
    /// Stabilní pro celou instalaci — Webshare podle něj počítá zařízení.
    device_uuid: String,
    token: RwLock<Option<String>>,
}

impl Webshare {
    pub fn new(http: reqwest::Client, device_uuid: impl Into<String>) -> Self {
        Self::with_base(http, device_uuid, BASE)
    }

    pub fn with_base(http: reqwest::Client, device_uuid: impl Into<String>, base: &str) -> Self {
        Self {
            http,
            base: base.trim_end_matches('/').to_owned(),
            device_uuid: device_uuid.into(),
            token: RwLock::new(None),
        }
    }

    fn wst(&self) -> String {
        self.token.read().unwrap().clone().unwrap_or_default()
    }

    async fn call(&self, endpoint: &str, params: &[(&str, &str)]) -> Result<String> {
        let res = self
            .http
            .post(format!("{}/{endpoint}/", self.base))
            .header("Accept", "text/xml; charset=UTF-8")
            .header("X-Requested-With", "XMLHttpRequest")
            .form(params)
            .send()
            .await?;

        let status = res.status();
        if !status.is_success() {
            return Err(Error::Http { status: status.as_u16(), context: format!("webshare {endpoint}") });
        }
        let body = res.text().await?;
        check_status(&body, endpoint)?;
        Ok(body)
    }

    /// Detail souboru včetně délky videa — s ní jde ověřit, že "film"
    /// o 90 minutách nemá 40 MB. Smazaný soubor vrací `Ok(None)`.
    ///
    /// Pozor: pod zátěží Webshare hlásí neexistenci i u živých souborů.
    /// `None` proto nikdy nebrat jako důvod soubor zapomenout napořád.
    pub async fn file_info(&self, ident: &str) -> Result<Option<RawFile>> {
        let wst = self.wst();
        match self.call("file_info", &[("ident", ident), ("wst", &wst)]).await {
            Ok(xml) => parse_file_info(&xml, ident).map(Some),
            Err(Error::Provider { .. }) => Ok(None),
            Err(e) => Err(e),
        }
    }
}

#[async_trait]
impl Provider for Webshare {
    fn id(&self) -> ProviderId {
        ProviderId::Webshare
    }

    fn max_concurrency(&self) -> usize {
        3
    }

    fn can_resolve(&self) -> bool {
        self.token.read().unwrap().is_some()
    }

    /// Dvoukrokový login: nejdřív salt uživatele, pak derivovaný hash —
    /// heslo v čitelné podobě neodchází nikdy.
    async fn login(&self, username: &str, password: &str) -> Result<String> {
        let xml = self.call("salt", &[("username_or_email", username), ("wst", "")]).await?;
        let salt = field(&xml, "salt")?.unwrap_or_default();

        let (hash, digest) = login_hashes(username, password, &salt);
        let xml = self
            .call(
                "login",
                &[
                    ("username_or_email", username),
                    ("password", &hash),
                    ("digest", &digest),
                    ("keep_logged_in", "1"),
                ],
            )
            .await?;

        let token = field(&xml, "token")?.filter(|t| !t.is_empty()).ok_or_else(|| {
            Error::BadResponse("Webshare nevrátil token".into())
        })?;
        *self.token.write().unwrap() = Some(token.clone());
        Ok(token)
    }

    fn set_token(&self, token: Option<String>) {
        *self.token.write().unwrap() = token;
    }

    fn token(&self) -> Option<String> {
        self.token.read().unwrap().clone()
    }

    async fn account(&self) -> Result<Account> {
        let wst = self.token().ok_or(Error::NotLoggedIn)?;
        let xml = self.call("user_data", &[("wst", &wst)]).await?;
        parse_account(&xml)
    }

    async fn search(&self, query: &str, opts: &SearchOptions) -> Result<Vec<RawFile>> {
        let sort = match opts.sort {
            SearchSort::Relevance => "",
            SearchSort::Recent => "recent",
            SearchSort::Rating => "rating",
            SearchSort::Largest => "largest",
        };
        let (limit, offset, wst) = (opts.limit.to_string(), opts.offset.to_string(), self.wst());
        let xml = self
            .call(
                "search",
                &[
                    ("what", query),
                    ("category", "video"),
                    ("sort", sort),
                    ("limit", &limit),
                    ("offset", &offset),
                    ("wst", &wst),
                ],
            )
            .await?;
        parse_search(&xml)
    }

    async fn resolve(&self, ident: &str, purpose: LinkPurpose) -> Result<ResolvedLink> {
        let wst = self.token().ok_or(Error::NotLoggedIn)?;
        // `video_stream` vrací přímý odkaz na celý soubor, takže poslouží
        // pro přehrávání i stažení.
        let xml = self
            .call(
                "file_link",
                &[
                    ("ident", ident),
                    ("wst", &wst),
                    ("download_type", link_type(purpose)),
                    ("device_uuid", &self.device_uuid),
                    ("force_https", "1"),
                ],
            )
            .await?;
        let url = field(&xml, "link")?
            .filter(|u| !u.is_empty())
            .ok_or_else(|| Error::BadResponse("Webshare nevrátil odkaz".into()))?;
        Ok(ResolvedLink {
            url,
            headers: vec![],
            expires_at_ms: Some(crate::time::now_ms() + LINK_TTL_MS),
            subtitles: vec![],
        })
    }
}

/// Typ odkazu u Webshare. Přepsatelný proměnnou `KURO_WS_LINK_TYPE` (jen
/// pro měření rychlosti v CLI).
fn link_type(purpose: LinkPurpose) -> &'static str {
    match std::env::var("KURO_WS_LINK_TYPE").as_deref() {
        Ok("file_download") => "file_download",
        Ok("video_stream") => "video_stream",
        _ => match purpose {
            LinkPurpose::Stream => "video_stream",
            LinkPurpose::Download => "file_download",
        },
    }
}

/* ------------------------------ hashování ------------------------------ */

/// `password = sha1(md5crypt(heslo, salt))`, `digest = md5("jméno:Webshare:password")`.
/// Pořadí a formát určuje Webshare.
pub fn login_hashes(username: &str, password: &str, salt: &str) -> (String, String) {
    let hash = hex(&Sha1::digest(md5crypt(password, salt).as_bytes()));
    let digest = hex(&Md5::digest(format!("{username}:Webshare:{hash}").as_bytes()));
    (hash, digest)
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/* ------------------------------ parsování XML ------------------------------ */

fn parse(xml: &str) -> Result<Document<'_>> {
    Document::parse(xml).map_err(|e| Error::BadResponse(format!("Webshare XML: {e}")))
}

fn child<'a>(node: Node<'a, 'a>, name: &str) -> Option<Node<'a, 'a>> {
    node.children().find(|c| c.has_tag_name(name))
}

fn child_text(node: Node<'_, '_>, name: &str) -> Option<String> {
    child(node, name).and_then(|c| c.text()).map(|t| t.trim().to_owned())
}

fn child_num<T: std::str::FromStr + Default>(node: Node<'_, '_>, name: &str) -> T {
    child_text(node, name).and_then(|t| t.parse().ok()).unwrap_or_default()
}

/// Text přímého potomka `<response>`.
fn field(xml: &str, name: &str) -> Result<Option<String>> {
    Ok(child_text(parse(xml)?.root_element(), name))
}

/// Webshare kódy, které znamenají propadlý nebo neplatný token.
const AUTH_CODES: &[&str] = &["NOT_LOGGED_IN", "INVALID_TOKEN", "FILE_LINK_FATAL_2"];

pub(crate) fn check_status(xml: &str, endpoint: &str) -> Result<()> {
    let doc = parse(xml)?;
    let root = doc.root_element();
    if !root.has_tag_name("response") {
        return Err(Error::BadResponse(format!("Webshare {endpoint}: chybí <response>")));
    }
    let status = child_text(root, "status").unwrap_or_default();
    if status.is_empty() || status == "OK" {
        return Ok(());
    }
    let code = child_text(root, "code").unwrap_or_else(|| "UNKNOWN".into());
    let message = child_text(root, "message").unwrap_or_else(|| format!("Webshare odmítl {endpoint}"));

    if AUTH_CODES.contains(&code.as_str()) {
        return Err(Error::AuthExpired);
    }
    if code == "LOGIN_FATAL_1" || (endpoint == "salt" && code.starts_with("SALT_FATAL")) {
        return Err(Error::BadCredentials);
    }
    Err(Error::Provider { provider: "webshare".into(), code, message })
}

pub(crate) fn parse_search(xml: &str) -> Result<Vec<RawFile>> {
    let doc = parse(xml)?;
    Ok(doc
        .root_element()
        .children()
        .filter(|n| n.has_tag_name("file"))
        .map(|f| RawFile {
            ident: child_text(f, "ident").unwrap_or_default(),
            name: child_text(f, "name").unwrap_or_default(),
            size_bytes: child_num(f, "size"),
            positive_votes: child_num(f, "positive_votes"),
            negative_votes: child_num(f, "negative_votes"),
            password_protected: child_text(f, "password").as_deref() == Some("1"),
            // Výsledky hledání délku nenesou; bere se z file_info.
            duration_sec: None,
            max_height: None,
            dedup_key: None,
        })
        .filter(|f| !f.ident.is_empty())
        .collect())
}

pub(crate) fn parse_file_info(xml: &str, ident: &str) -> Result<RawFile> {
    let doc = parse(xml)?;
    let r = doc.root_element();
    // <video><stream><duration>7245</duration></stream></video> — struktura
    // se liší podle souboru, proto s fallbacky.
    let video = child(r, "video");
    let duration = video
        .and_then(|v| child(v, "stream"))
        .and_then(|s| child_text(s, "duration"))
        .or_else(|| video.and_then(|v| child_text(v, "length")))
        .or_else(|| child_text(r, "length"))
        .and_then(|d| d.parse::<f64>().ok())
        .filter(|d| *d > 0.0)
        .map(|d| d as u32);

    Ok(RawFile {
        ident: ident.to_owned(),
        name: child_text(r, "name").unwrap_or_default(),
        size_bytes: child_num(r, "size"),
        positive_votes: child_num(r, "positive_votes"),
        negative_votes: child_num(r, "negative_votes"),
        password_protected: child_text(r, "password").as_deref() == Some("1"),
        duration_sec: duration,
        max_height: None,
        dedup_key: None,
    })
}

pub(crate) fn parse_account(xml: &str) -> Result<Account> {
    let doc = parse(xml)?;
    let r = doc.root_element();
    Ok(Account {
        username: child_text(r, "username").filter(|s| !s.is_empty()),
        vip: child_text(r, "vip").as_deref() == Some("1"),
        vip_until: child_text(r, "vip_until").filter(|s| !s.is_empty()),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hashes_are_deterministic_hex() {
        let (h, d) = login_hashes("user", "password", "salt");
        assert_eq!(h.len(), 40);
        assert_eq!(d.len(), 32);
        // sha1("$1$salt$qJH7.N4xYta3aEG/dfqo/0")
        assert_eq!(h, hex(&Sha1::digest(b"$1$salt$qJH7.N4xYta3aEG/dfqo/0")));
    }

    #[test]
    fn status_errors() {
        let fatal = "<?xml version=\"1.0\"?><response><status>FATAL</status><code>NOT_LOGGED_IN</code><message>x</message></response>";
        assert!(matches!(check_status(fatal, "file_link"), Err(Error::AuthExpired)));

        let bad = "<response><status>FATAL</status><code>LOGIN_FATAL_1</code></response>";
        assert!(matches!(check_status(bad, "login"), Err(Error::BadCredentials)));

        let other = "<response><status>FATAL</status><code>FILE_INFO_FATAL_1</code><message>File not found.</message></response>";
        match check_status(other, "file_info") {
            Err(Error::Provider { code, .. }) => assert_eq!(code, "FILE_INFO_FATAL_1"),
            r => panic!("{r:?}"),
        }

        assert!(check_status("<response><status>OK</status></response>", "x").is_ok());
    }

    #[test]
    fn parses_search() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<response><status>OK</status><total>2</total>
  <file><ident>abc123</ident><name>Interstellar.2014.1080p.CZ.mkv</name><size>4294967296</size>
        <positive_votes>12</positive_votes><negative_votes>1</negative_votes><password>0</password></file>
  <file><ident>def456</ident><name>Locked.mkv</name><size>100</size><password>1</password></file>
  <file><ident></ident><name>broken</name></file>
</response>"#;
        let files = parse_search(xml).unwrap();
        assert_eq!(files.len(), 2);
        assert_eq!(files[0].ident, "abc123");
        assert_eq!(files[0].size_bytes, 4_294_967_296);
        assert_eq!(files[0].positive_votes, 12);
        assert!(!files[0].password_protected);
        assert!(files[1].password_protected);
    }

    #[test]
    fn parses_file_info_duration() {
        let xml = "<response><status>OK</status><name>a.mkv</name><size>10</size><video><stream><duration>7245</duration></stream></video></response>";
        assert_eq!(parse_file_info(xml, "x").unwrap().duration_sec, Some(7245));
        let xml = "<response><status>OK</status><name>a.mkv</name><length>60.5</length></response>";
        assert_eq!(parse_file_info(xml, "x").unwrap().duration_sec, Some(60));
    }

    #[test]
    fn parses_account() {
        let xml = "<response><status>OK</status><username>kuro</username><vip>1</vip><vip_until>2027-01-01 12:00:00</vip_until></response>";
        let a = parse_account(xml).unwrap();
        assert_eq!(a.username.as_deref(), Some("kuro"));
        assert!(a.vip);
        assert_eq!(a.vip_until.as_deref(), Some("2027-01-01 12:00:00"));
    }
}
