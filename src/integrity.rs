//! Refusing a modified Roblox.
//!
//! The client is checked the way macOS checks it before running it: the
//! signature on the main executable's code directory has to come from Roblox's
//! Developer ID, issued by Apple's Developer ID authority under Apple's root;
//! every page of every executable has to hash to what its code directory says;
//! and every other file has to match the seal in `_CodeSignature/CodeResources`.
//! A file that was changed, removed or added fails the check. The one file let
//! in besides is the `ClientAppSettings.json` Ginrose writes, and only with
//! allowlisted flags in it.
//!
//! The Darwin system files in the Rosemary prefix are what the client links
//! against, so they are checked too, by Rosemary against the base its own build
//! pins.

use std::collections::{BTreeMap, HashSet};
use std::path::Path;
use std::process::{Command, Stdio};

use cms::cert::CertificateChoices;
use cms::cert::x509::Certificate;
use cms::content_info::ContentInfo;
use cms::signed_data::{SignedData, SignerIdentifier};
use der::asn1::{ObjectIdentifier, OctetString, PrintableStringRef, Utf8StringRef};
use der::{Decode, Encode};
use rsa::pkcs1::DecodeRsaPublicKey;
use rsa::{Pkcs1v15Sign, RsaPublicKey};
use sha2::{Digest, Sha256};

use crate::{flags, paths};

/// Roblox Corporation's Apple team, in the subject of its Developer ID.
const TEAM: &str = "2CFABCH843";
const IDENTIFIER: &str = "com.roblox.RobloxPlayer";
/// Apple Root CA, as apple.com/certificateauthority publishes it (SHA-256
/// B0B1730E…01F024). The chain is checked against this copy, never against
/// the root a signature carries.
const APPLE_ROOT: &[u8] = include_bytes!("../assets/apple-root-ca.cer");
/// Files Ginrose itself writes into the bundle, relative to `Contents/`.
const SETTINGS_FILE: &str = "MacOS/ClientSettings/ClientAppSettings.json";

const SIGNED_DATA: ObjectIdentifier = ObjectIdentifier::new_unwrap("1.2.840.113549.1.7.2");
const MESSAGE_DIGEST: ObjectIdentifier = ObjectIdentifier::new_unwrap("1.2.840.113549.1.9.4");
const SHA256: ObjectIdentifier = ObjectIdentifier::new_unwrap("2.16.840.1.101.3.4.2.1");
const RSA: ObjectIdentifier = ObjectIdentifier::new_unwrap("1.2.840.113549.1.1.1");
const SHA256_RSA: ObjectIdentifier = ObjectIdentifier::new_unwrap("1.2.840.113549.1.1.11");
const ORGANIZATIONAL_UNIT: ObjectIdentifier = ObjectIdentifier::new_unwrap("2.5.4.11");
/// Marks Apple's Developer ID intermediate authority.
const DEVELOPER_ID_CA: ObjectIdentifier = ObjectIdentifier::new_unwrap("1.2.840.113635.100.6.2.6");
/// Marks a Developer ID Application certificate.
const DEVELOPER_ID_APP: ObjectIdentifier = ObjectIdentifier::new_unwrap("1.2.840.113635.100.6.1.13");

const CPU_X86_64: u32 = 0x0100_0007;
const LC_CODE_SIGNATURE: u32 = 0x1d;
const SUPERBLOB: u32 = 0xfade_0cc0;
const CODE_DIRECTORY: u32 = 0xfade_0c02;
const SLOT_INFO: u32 = 1;
const SLOT_REQUIREMENTS: u32 = 2;
const SLOT_RESOURCES: u32 = 3;
const SLOT_ENTITLEMENTS: u32 = 5;
const SLOT_DER_ENTITLEMENTS: u32 = 7;
const SLOT_SIGNATURE: u32 = 0x10000;

type Check<T = ()> = Result<T, String>;

fn sha256(bytes: &[u8]) -> [u8; 32] {
    Sha256::digest(bytes).into()
}

fn be32(bytes: &[u8], at: usize) -> Check<u32> {
    bytes
        .get(at..at + 4)
        .map(|b| u32::from_be_bytes(b.try_into().unwrap()))
        .ok_or_else(|| "truncated signature".to_string())
}

fn le32(bytes: &[u8], at: usize) -> Check<u32> {
    bytes
        .get(at..at + 4)
        .map(|b| u32::from_le_bytes(b.try_into().unwrap()))
        .ok_or_else(|| "truncated executable".to_string())
}

/// One signed executable: its code directory, and the other blobs its
/// signature holds, by slot.
struct Signed {
    directory: Vec<u8>,
    blobs: BTreeMap<u32, Vec<u8>>,
    identifier: String,
    team: String,
}

impl Signed {
    /// What a parent's seal names this code by: the directory's SHA-256, cut
    /// to 20 bytes.
    fn cdhash(&self) -> [u8; 20] {
        sha256(&self.directory)[..20].try_into().unwrap()
    }

    fn hash_size(&self) -> usize {
        self.directory[36] as usize
    }

    /// The hash a special slot holds, or `None` when it is empty.
    fn special(&self, slot: u32) -> Check<Option<&[u8]>> {
        let specials = be32(&self.directory, 24)?;
        if slot > specials {
            return Ok(None);
        }
        let size = self.hash_size();
        let offset = be32(&self.directory, 16)? as usize;
        let start = offset
            .checked_sub(slot as usize * size)
            .ok_or("bad code directory")?;
        let hash = self
            .directory
            .get(start..start + size)
            .ok_or("bad code directory")?;
        Ok((!hash.iter().all(|b| *b == 0)).then_some(hash))
    }
}

/// The architectures in a Mach-O file, each with its own code and signature.
fn slices(bytes: &[u8]) -> Check<Vec<(u32, &[u8])>> {
    match be32(bytes, 0)? {
        0xcafe_babe => (0..be32(bytes, 4)? as usize)
            .map(|index| {
                let at = 8 + index * 20;
                let offset = be32(bytes, at + 8)? as usize;
                let size = be32(bytes, at + 12)? as usize;
                let slice = bytes
                    .get(offset..offset + size)
                    .ok_or("truncated universal binary")?;
                Ok((be32(bytes, at)?, slice))
            })
            .collect(),
        0xcffa_edfe => Ok(vec![(le32(bytes, 4)?, bytes)]),
        _ => Err("not a Mach-O executable".into()),
    }
}

/// Where a slice's signature starts, and its blobs by slot.
fn superblob(slice: &[u8]) -> Check<(usize, BTreeMap<u32, Vec<u8>>)> {
    let commands = le32(slice, 16)? as usize;
    let mut at = 32;
    let mut signature = None;
    for _ in 0..commands {
        let (cmd, size) = (le32(slice, at)?, le32(slice, at + 4)? as usize);
        if cmd == LC_CODE_SIGNATURE {
            signature = Some((le32(slice, at + 8)? as usize, le32(slice, at + 12)? as usize));
        }
        if size < 8 {
            return Err("bad load command".into());
        }
        at += size;
    }
    let (offset, size) = signature.ok_or("not signed")?;
    if offset.checked_add(size) != Some(slice.len()) {
        return Err("data after the signature".into());
    }
    let superblob = &slice[offset..];
    if be32(superblob, 0)? != SUPERBLOB {
        return Err("bad signature".into());
    }
    let mut blobs = BTreeMap::new();
    for index in 0..be32(superblob, 8)? as usize {
        let slot = be32(superblob, 12 + index * 8)?;
        let start = be32(superblob, 16 + index * 8)? as usize;
        let length = be32(superblob, start + 4)? as usize;
        let blob = superblob
            .get(start..start + length)
            .ok_or("truncated signature")?;
        blobs.insert(slot, blob.to_vec());
    }
    Ok((offset, blobs))
}

/// A signed executable: its x86_64 code, checked page by page and signed by
/// Roblox, and the code directory hash of every architecture in it. A parent
/// seals one of those, often the arm64 one, which is why the x86_64 code needs
/// its own signature checked rather than relying on the seal.
struct Code {
    signed: Signed,
    cdhashes: Vec<[u8; 20]>,
}

fn code(bytes: &[u8]) -> Check<Code> {
    let mut cdhashes = Vec::new();
    let mut x86_64 = None;
    for (cpu, slice) in slices(bytes)? {
        if cpu == CPU_X86_64 {
            let signed = signed_code(slice)?;
            cdhashes.push(signed.cdhash());
            x86_64 = Some(signed);
        } else if let Some(directory) = superblob(slice)?.1.get(&0)
            && directory.get(37) == Some(&2)
        {
            cdhashes.push(sha256(directory)[..20].try_into().unwrap());
        }
    }
    let signed = x86_64.ok_or("no x86_64 code in it")?;
    let cms = signed.blobs.get(&SLOT_SIGNATURE).ok_or("not signed")?;
    signed_by_roblox(&signed.directory, cms.get(8..).ok_or("bad signature")?)?;
    Ok(Code { signed, cdhashes })
}

/// Read the signature of `slice` and check every page of the code against its
/// code directory.
fn signed_code(slice: &[u8]) -> Check<Signed> {
    let (offset, mut blobs) = superblob(slice)?;
    let directory = blobs.remove(&0).ok_or("no code directory")?;
    let cd = &directory;
    if be32(cd, 0)? != CODE_DIRECTORY {
        return Err("bad code directory".into());
    }
    let version = be32(cd, 8)?;
    let (hash_offset, code_slots) = (be32(cd, 16)? as usize, be32(cd, 28)? as usize);
    let code_limit = be32(cd, 32)? as usize;
    let (hash_size, hash_type, page_shift) = (cd[36] as usize, cd[37], cd[39]);
    if hash_type != 2 || hash_size != 32 || !(12..=16).contains(&page_shift) {
        return Err("code directory is not SHA-256 with pages".into());
    }
    if version >= 0x20100 && be32(cd, 44)? != 0 {
        return Err("scattered code directory".into());
    }
    if version >= 0x20300 && cd.get(56..64).is_some_and(|b| b.iter().any(|x| *x != 0)) {
        return Err("64-bit code limit".into());
    }
    if code_limit != offset {
        return Err("code directory does not cover the whole executable".into());
    }
    let page = 1usize << page_shift;
    if code_slots != code_limit.div_ceil(page) {
        return Err("code directory has the wrong number of pages".into());
    }
    for index in 0..code_slots {
        let start = index * page;
        let end = (start + page).min(code_limit);
        let want = cd
            .get(hash_offset + index * 32..hash_offset + index * 32 + 32)
            .ok_or("bad code directory")?;
        if sha256(&slice[start..end]) != want {
            return Err(format!("code page {index} was changed"));
        }
    }
    let string_at = |at: usize| -> String {
        cd.get(at..)
            .and_then(|rest| rest.split(|b| *b == 0).next())
            .map(|s| String::from_utf8_lossy(s).into_owned())
            .unwrap_or_default()
    };
    let identifier = string_at(be32(cd, 20)? as usize);
    let team = match version >= 0x20200 {
        true => match be32(cd, 48)? {
            0 => String::new(),
            at => string_at(at as usize),
        },
        false => String::new(),
    };
    let signed = Signed {
        directory,
        blobs,
        identifier,
        team,
    };
    for slot in [SLOT_REQUIREMENTS, SLOT_ENTITLEMENTS, SLOT_DER_ENTITLEMENTS] {
        match (signed.blobs.get(&slot), signed.special(slot)?) {
            (Some(blob), Some(hash)) if sha256(blob) == hash => {}
            (None, None) => {}
            _ => return Err(format!("signature slot {slot} was changed")),
        }
    }
    for slot in [4, 6] {
        if signed.special(slot)?.is_some() {
            return Err(format!("unknown sealed slot {slot}"));
        }
    }
    if signed.team != TEAM {
        return Err(format!("signed by team {:?}, not Roblox", signed.team));
    }
    Ok(signed)
}

fn extension(cert: &Certificate, oid: ObjectIdentifier) -> bool {
    cert.tbs_certificate
        .extensions
        .iter()
        .flatten()
        .any(|e| e.extn_id == oid)
}

fn organizational_units(cert: &Certificate) -> Vec<String> {
    cert.tbs_certificate
        .subject
        .0
        .iter()
        .flat_map(|rdn| rdn.0.iter())
        .filter(|atv| atv.oid == ORGANIZATIONAL_UNIT)
        .filter_map(|atv| {
            atv.value
                .decode_as::<Utf8StringRef>()
                .map(|s| s.to_string())
                .or_else(|_| atv.value.decode_as::<PrintableStringRef>().map(|s| s.to_string()))
                .ok()
        })
        .collect()
}

fn rsa_key(cert: &Certificate) -> Check<RsaPublicKey> {
    let spki = &cert.tbs_certificate.subject_public_key_info;
    if spki.algorithm.oid != RSA {
        return Err("certificate key is not RSA".into());
    }
    let bits = spki
        .subject_public_key
        .as_bytes()
        .ok_or("bad certificate key")?;
    RsaPublicKey::from_pkcs1_der(bits).map_err(|e| format!("bad certificate key: {e}"))
}

/// Whether `issuer`'s key made `cert`'s signature.
fn issued_by(cert: &Certificate, issuer: &Certificate) -> Check {
    if cert.tbs_certificate.issuer != issuer.tbs_certificate.subject {
        return Err("certificate chain does not link up".into());
    }
    if cert.signature_algorithm.oid != SHA256_RSA {
        return Err("certificate is not signed with SHA-256 and RSA".into());
    }
    let tbs = cert.tbs_certificate.to_der().map_err(|e| e.to_string())?;
    let signature = cert.signature.as_bytes().ok_or("bad certificate signature")?;
    rsa_key(issuer)?
        .verify(Pkcs1v15Sign::new::<Sha256>(), &sha256(&tbs), signature)
        .map_err(|_| "certificate signature does not verify".to_string())
}

fn push_length(out: &mut Vec<u8>, length: usize) {
    if length < 0x80 {
        out.push(length as u8);
    } else {
        let bytes = length.to_be_bytes();
        let skip = bytes.iter().take_while(|b| **b == 0).count();
        out.push(0x80 | (bytes.len() - skip) as u8);
        out.extend_from_slice(&bytes[skip..]);
    }
}

/// Copy one BER element from the front of `input` into `out` with definite
/// lengths; returns how many bytes it took up.
fn definite_element(input: &[u8], out: &mut Vec<u8>, depth: usize) -> Check<usize> {
    let bad = || "bad signature encoding".to_string();
    if depth > 32 {
        return Err(bad());
    }
    let mut at = 1;
    if *input.first().ok_or_else(bad)? & 0x1f == 0x1f {
        while *input.get(at).ok_or_else(bad)? & 0x80 != 0 {
            at += 1;
        }
        at += 1;
    }
    let tag = &input[..at];
    let constructed = input[0] & 0x20 != 0;
    let first = *input.get(at).ok_or_else(bad)?;
    at += 1;
    let mut body = Vec::new();
    if first == 0x80 {
        if !constructed {
            return Err(bad());
        }
        while !input.get(at..at + 2).ok_or_else(bad)?.eq(&[0, 0]) {
            at += definite_element(&input[at..], &mut body, depth + 1)?;
        }
        at += 2;
    } else {
        let length = match first {
            0..0x80 => first as usize,
            _ => {
                let count = (first & 0x7f) as usize;
                if count > 4 {
                    return Err(bad());
                }
                let bytes = input.get(at..at + count).ok_or_else(bad)?;
                at += count;
                bytes.iter().fold(0usize, |n, b| n << 8 | *b as usize)
            }
        };
        let content = input.get(at..at + length).ok_or_else(bad)?;
        at += length;
        if constructed {
            let mut inner = 0;
            while inner < content.len() {
                inner += definite_element(&content[inner..], &mut body, depth + 1)?;
            }
        } else {
            body.extend_from_slice(content);
        }
    }
    out.extend_from_slice(tag);
    push_length(out, body.len());
    out.extend_from_slice(&body);
    Ok(at)
}

/// Check that the CMS signature over `directory` comes from Roblox's Developer
/// ID through Apple's root.
fn signed_by_roblox(directory: &[u8], cms: &[u8]) -> Check {
    // codesign writes BER with indefinite lengths, which DER parsers refuse.
    let mut der = Vec::new();
    definite_element(cms, &mut der, 0)?;
    let info = ContentInfo::from_der(&der).map_err(|e| format!("bad signature: {e}"))?;
    if info.content_type != SIGNED_DATA {
        return Err("bad signature".into());
    }
    let data: SignedData = info
        .content
        .decode_as()
        .map_err(|e| format!("bad signature: {e}"))?;
    let [signer] = data.signer_infos.0.as_slice() else {
        return Err("not exactly one signer".into());
    };
    if signer.digest_alg.oid != SHA256 {
        return Err("signature digest is not SHA-256".into());
    }
    let attributes = signer.signed_attrs.as_ref().ok_or("no signed attributes")?;
    let digest = attributes
        .iter()
        .find(|a| a.oid == MESSAGE_DIGEST)
        .and_then(|a| a.values.iter().next())
        .and_then(|v| v.decode_as::<OctetString>().ok())
        .ok_or("no message digest")?;
    if digest.as_bytes() != sha256(directory) {
        return Err("signature is for a different code directory".into());
    }
    if ![RSA, SHA256_RSA].contains(&signer.signature_algorithm.oid) {
        return Err("signature is not RSA".into());
    }
    let certificates: Vec<&Certificate> = data
        .certificates
        .iter()
        .flat_map(|set| set.0.iter())
        .filter_map(|choice| match choice {
            CertificateChoices::Certificate(cert) => Some(cert),
            _ => None,
        })
        .collect();
    let SignerIdentifier::IssuerAndSerialNumber(id) = &signer.sid else {
        return Err("signer named by key identifier".into());
    };
    let leaf = certificates
        .iter()
        .find(|c| {
            c.tbs_certificate.issuer == id.issuer
                && c.tbs_certificate.serial_number == id.serial_number
        })
        .ok_or("signer certificate missing")?;
    let signed = attributes.to_der().map_err(|e| e.to_string())?;
    rsa_key(leaf)?
        .verify(
            Pkcs1v15Sign::new::<Sha256>(),
            &sha256(&signed),
            signer.signature.as_bytes(),
        )
        .map_err(|_| "signature does not verify".to_string())?;
    let intermediate = certificates
        .iter()
        .find(|c| c.tbs_certificate.subject == leaf.tbs_certificate.issuer)
        .ok_or("Developer ID authority missing")?;
    issued_by(leaf, intermediate)?;
    let root = Certificate::from_der(APPLE_ROOT).map_err(|e| e.to_string())?;
    issued_by(intermediate, &root)?;
    if !extension(intermediate, DEVELOPER_ID_CA) {
        return Err("not issued by Apple's Developer ID authority".into());
    }
    if !extension(leaf, DEVELOPER_ID_APP) {
        return Err("not a Developer ID Application certificate".into());
    }
    if !organizational_units(leaf).iter().any(|ou| ou == TEAM) {
        return Err("Developer ID is not Roblox's".into());
    }
    Ok(())
}

/// A rule from `rules2`: files whose path matches the highest-weighted pattern
/// are sealed, sealed but optional, or left out.
struct Rule {
    pattern: regex::Regex,
    weight: f64,
    omit: bool,
}

struct Seal {
    files: plist::Dictionary,
    rules: Vec<Rule>,
}

impl Seal {
    fn parse(bytes: &[u8]) -> Check<Seal> {
        let value = plist::Value::from_reader(std::io::Cursor::new(bytes))
            .map_err(|e| format!("bad CodeResources: {e}"))?;
        let mut top = value.into_dictionary().ok_or("bad CodeResources")?;
        let files = top
            .remove("files2")
            .and_then(plist::Value::into_dictionary)
            .ok_or("CodeResources has no files2")?;
        let mut rules = Vec::new();
        for (pattern, rule) in top
            .remove("rules2")
            .and_then(plist::Value::into_dictionary)
            .ok_or("CodeResources has no rules2")?
        {
            let pattern = regex::Regex::new(&pattern).map_err(|e| e.to_string())?;
            let (weight, omit) = match rule.as_dictionary() {
                Some(d) => (
                    d.get("weight").and_then(plist::Value::as_real).unwrap_or(1.0),
                    d.get("omit").and_then(plist::Value::as_boolean).unwrap_or(false),
                ),
                None => (1.0, false),
            };
            rules.push(Rule {
                pattern,
                weight,
                omit,
            });
        }
        Ok(Seal { files, rules })
    }

    /// Whether a file the seal does not name may be there anyway.
    fn omitted(&self, path: &str) -> bool {
        self.rules
            .iter()
            .filter(|r| r.pattern.is_match(path))
            .max_by(|a, b| a.weight.total_cmp(&b.weight))
            .is_some_and(|r| r.omit)
    }
}

fn walk(root: &Path, dir: &Path, out: &mut Vec<(String, std::fs::FileType)>) -> Check {
    let entries = std::fs::read_dir(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    for entry in entries {
        let entry = entry.map_err(|e| e.to_string())?;
        let kind = entry.file_type().map_err(|e| e.to_string())?;
        let path = entry.path();
        let relative = path
            .strip_prefix(root)
            .unwrap()
            .to_string_lossy()
            .into_owned();
        if kind.is_dir() {
            walk(root, &path, out)?;
        } else {
            out.push((relative, kind));
        }
    }
    Ok(())
}

fn executable_name(contents: &Path) -> Check<String> {
    let info = plist::Value::from_file(contents.join("Info.plist"))
        .map_err(|e| format!("bad Info.plist: {e}"))?;
    info.as_dictionary()
        .and_then(|d| d.get("CFBundleExecutable"))
        .and_then(plist::Value::as_string)
        .map(str::to_string)
        .ok_or_else(|| "Info.plist names no executable".into())
}

/// Check a bundle's executable and everything its seal covers; returns the
/// executable's code, for the parent to compare with its own seal.
fn bundle(contents: &Path, depth: usize) -> Check<Code> {
    if depth > 4 {
        return Err("bundles nested too deep".into());
    }
    let executable = executable_name(contents)?;
    if executable.contains('/') {
        return Err("bad executable name".into());
    }
    let main_relative = format!("MacOS/{executable}");
    let bytes = std::fs::read(contents.join(&main_relative))
        .map_err(|e| format!("{main_relative}: {e}"))?;
    let main = code(&bytes).map_err(|e| format!("{main_relative}: {e}"))?;
    drop(bytes);
    let signed = &main.signed;
    let info = std::fs::read(contents.join("Info.plist")).map_err(|e| e.to_string())?;
    if signed.special(SLOT_INFO)? != Some(&sha256(&info)[..]) {
        return Err("Info.plist was changed".into());
    }
    let resources = std::fs::read(contents.join("_CodeSignature/CodeResources"))
        .map_err(|e| format!("CodeResources: {e}"))?;
    if signed.special(SLOT_RESOURCES)? != Some(&sha256(&resources)[..]) {
        return Err("CodeResources was changed".into());
    }
    let seal = Seal::parse(&resources)?;

    let mut nested_bundles = Vec::new();
    for (path, entry) in &seal.files {
        let Some(cdhash) = entry
            .as_dictionary()
            .and_then(|d| d.get("cdhash"))
            .and_then(plist::Value::as_data)
        else {
            continue;
        };
        let at = contents.join(path);
        let nested = if at.is_dir() {
            nested_bundles.push(format!("{path}/"));
            bundle(&at.join("Contents"), depth + 1).map_err(|e| format!("{path}: {e}"))?
        } else {
            let bytes = std::fs::read(&at).map_err(|e| format!("{path}: {e}"))?;
            code(&bytes).map_err(|e| format!("{path}: {e}"))?
        };
        if !nested.cdhashes.iter().any(|h| h[..] == *cdhash) {
            return Err(format!("{path} is not the code that was sealed"));
        }
    }

    let mut found = Vec::new();
    walk(contents, contents, &mut found)?;
    let mut seen = HashSet::new();
    for (path, kind) in found {
        if path == main_relative
            || path == "_CodeSignature/CodeResources"
            || (depth == 0 && path == SETTINGS_FILE && kind.is_file())
            || nested_bundles.iter().any(|b| path.starts_with(b.as_str()))
        {
            continue;
        }
        let Some(entry) = seal.files.get(&path) else {
            if seal.omitted(&path) {
                continue;
            }
            return Err(format!("{path} was added"));
        };
        seen.insert(path.clone());
        let entry = entry.as_dictionary();
        let at = contents.join(&path);
        if entry.is_some_and(|d| d.contains_key("cdhash")) {
            continue;
        }
        if let Some(target) = entry.and_then(|d| d.get("symlink")).and_then(plist::Value::as_string) {
            let link = std::fs::read_link(&at).map_err(|e| format!("{path}: {e}"))?;
            if !kind.is_symlink() || link != Path::new(target) {
                return Err(format!("{path} was changed"));
            }
            continue;
        }
        let want = entry
            .and_then(|d| d.get("hash2"))
            .and_then(plist::Value::as_data)
            .ok_or_else(|| format!("{path} has no SHA-256 in the seal"))?;
        if !kind.is_file() {
            return Err(format!("{path} was changed"));
        }
        let bytes = std::fs::read(&at).map_err(|e| format!("{path}: {e}"))?;
        if sha256(&bytes)[..] != *want {
            return Err(format!("{path} was changed"));
        }
    }
    for (path, entry) in &seal.files {
        let entry = entry.as_dictionary();
        let nested = entry.is_some_and(|d| d.contains_key("cdhash"));
        let optional = entry
            .and_then(|d| d.get("optional"))
            .and_then(plist::Value::as_boolean)
            .unwrap_or(false);
        if !nested && !optional && !seen.contains(path) {
            return Err(format!("{path} was removed"));
        }
    }
    Ok(main)
}

/// The flags file may hold allowlisted flags and nothing else.
fn settings_file(contents: &Path) -> Check {
    let path = contents.join(SETTINGS_FILE);
    let meta = match std::fs::symlink_metadata(&path) {
        Ok(meta) => meta,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(e.to_string()),
    };
    if !meta.is_file() {
        return Err("ClientAppSettings.json is not a plain file".into());
    }
    let text = std::fs::read(&path).map_err(|e| e.to_string())?;
    let flags: BTreeMap<String, serde_json::Value> = serde_json::from_slice(&text)
        .map_err(|_| "ClientAppSettings.json is not a flag list".to_string())?;
    match flags.keys().find(|name| flags::find(name).is_none()) {
        Some(name) => Err(format!("ClientAppSettings.json sets {name}, which is not allowlisted")),
        None => Ok(()),
    }
}

/// Check the client bundle at `app` (a `Roblox.app`).
pub fn client(app: &Path) -> Check {
    let contents = app.join("Contents");
    let main = bundle(&contents, 0)?;
    if main.signed.identifier != IDENTIFIER {
        return Err(format!("the app is {:?}, not Roblox", main.signed.identifier));
    }
    settings_file(&contents)
}

fn rosemary_output(rosemary: &Path, args: &[&str]) -> Check<(bool, String)> {
    let out = Command::new(rosemary)
        .arg("prefix")
        .args(args)
        .arg(paths::prefix())
        .env("ROSEMARY_PREFIX_SETUP", "download")
        .stdin(Stdio::null())
        .output()
        .map_err(|e| format!("could not run Rosemary: {e}"))?;
    let mut text = String::from_utf8_lossy(&out.stdout).into_owned();
    text.push_str(&String::from_utf8_lossy(&out.stderr));
    Ok((out.status.success(), text))
}

/// Whether the prefix is on the base this Rosemary pins, so that its own
/// manifest, which `verify` checks against, is not one handed to it.
fn on_pinned_base(rosemary: &Path) -> Check<bool> {
    let (_, info) = rosemary_output(rosemary, &["info"])?;
    Ok(info
        .lines()
        .any(|l| l.starts_with("pinned:") && l.trim_end().ends_with("up to date")))
}

/// Check the prefix's system files. `may_update` lets this move the prefix to
/// a newer pinned base first, which is only safe while nothing runs in it.
pub fn prefix(rosemary: &Path, may_update: bool) -> Check {
    if !on_pinned_base(rosemary)? {
        if !may_update {
            return Err("the Rosemary prefix needs an update; close Roblox and try again".into());
        }
        let (ok, text) = rosemary_output(rosemary, &["update"])?;
        if !ok || !on_pinned_base(rosemary)? {
            return Err(format!(
                "the Rosemary prefix is not on the base this Rosemary pins: {}",
                text.trim()
            ));
        }
    }
    let (ok, text) = rosemary_output(rosemary, &["verify"])?;
    if ok {
        return Ok(());
    }
    let detail = text
        .lines()
        .find(|l| l.contains("entries checked"))
        .map_or_else(|| text.trim().to_string(), |l| l.trim().to_string());
    Err(format!("the Rosemary prefix's system files were changed ({detail})"))
}

/// Put the prefix's system files back the way the base has them.
pub fn repair_prefix(rosemary: &Path) -> Check {
    let (ok, text) = rosemary_output(rosemary, &["repair"])?;
    match ok {
        true => Ok(()),
        false => Err(text.trim().to_string()),
    }
}

/// Environment variables that load code into Rosemary or the client, or point
/// them at other files: never passed on.
pub fn loads_code(name: &str) -> bool {
    name.starts_with("DYLD_")
        || name.starts_with("LD_")
        || matches!(
            name,
            "VK_LAYER_PATH"
                | "VK_ADD_LAYER_PATH"
                | "VK_IMPLICIT_LAYER_PATH"
                | "VK_ADD_IMPLICIT_LAYER_PATH"
                | "VK_INSTANCE_LAYERS"
                | "VK_LOADER_LAYERS_ENABLE"
                | "VK_ICD_FILENAMES"
                | "VK_DRIVER_FILES"
                | "VK_ADD_DRIVER_FILES"
                | "LIBGL_DRIVERS_PATH"
                | "__EGL_VENDOR_LIBRARY_FILENAMES"
                | "__EGL_VENDOR_LIBRARY_DIRS"
                | "SDL_DYNAMIC_API"
        )
}

/// Rosemary settings a user may add by hand. Every other `ROSEMARY_` variable
/// is Ginrose's to set, or a debugging hook.
const USER_ROSEMARY: &[&str] = &[
    "ROSEMARY_CONFINE_POINTER",
    "ROSEMARY_CONTROL_EDIT_KEYS",
    "ROSEMARY_DECORATIONS",
    "ROSEMARY_FRAME_INTERVAL",
    "ROSEMARY_GUI_SCALE",
    "ROSEMARY_METAL_MAX_COMMAND_BUFFERS",
    "ROSEMARY_METAL_PRESENTATION_MODE",
    "ROSEMARY_PLAIN_CTRL_CLICK",
    "ROSEMARY_SCREEN_REFRESH_RATE",
    "ROSEMARY_STATS",
    "ROSEMARY_SWAP_CTRL_CMD",
    "ROSEMARY_SWAP_INTERVAL",
    "ROSEMARY_WINDOW_SIZE",
];

/// Whether a variable from the user's extra environment may be passed on.
pub fn user_variable_allowed(name: &str) -> bool {
    !loads_code(name) && (!name.starts_with("ROSEMARY_") || USER_ROSEMARY.contains(&name))
}

pub fn explain(problem: &str) -> String {
    format!(
        "Ginrose only runs Roblox exactly as Roblox signed it, and this install was modified: {problem}. Run `ginrose repair` to download a clean copy."
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn installed() -> Option<std::path::PathBuf> {
        crate::roblox::installed().map(|b| b.app())
    }

    #[test]
    fn the_installed_client_passes() {
        let Some(app) = installed() else {
            return;
        };
        client(&app).unwrap();
    }

    #[test]
    fn code_loading_variables_are_refused() {
        assert!(!user_variable_allowed("DYLD_INSERT_LIBRARIES"));
        assert!(!user_variable_allowed("LD_PRELOAD"));
        assert!(!user_variable_allowed("ROSEMARY_DYLD_INSERT_LIBRARIES"));
        assert!(!user_variable_allowed("ROSEMARY_ENVIRON"));
        assert!(!user_variable_allowed("VK_ADD_LAYER_PATH"));
        assert!(user_variable_allowed("ROSEMARY_WINDOW_SIZE"));
        assert!(user_variable_allowed("MANGOHUD_CONFIG"));
    }
}
