//! U4Ai's update-signing key, in Sparkle's formats: the private key file
//! holds the base64 Ed25519 seed (what the Mac's `generate_keys -x` exports
//! and `sign_update --ed-key-file` reads), and the public key is the base64
//! string `u4ai_core::UPDATE_PUBLIC_KEY`, which is also the Mac app's
//! `SUPublicEDKey`, so one key signs all three systems' updates. Ed25519
//! signatures are deterministic, so signing a file here or with Sparkle on
//! the Mac gives the same signature.
//!
//!   cargo run --example update_key -- new <private-key-file>
//!   cargo run --example update_key -- public <private-key-file>
//!   cargo run --example update_key -- trusted
//!   cargo run --example update_key -- sign <file> <private-key-file>
//!   cargo run --example update_key -- verify <file> <base64 signature>
//!
//! `verify` checks against the key the updaters trust, which `trusted`
//! prints; windows/scripts/release.ps1 compares it with `public`, and
//! make-appcast.ps1 and linux/scripts/make-appcast.sh run `sign` and
//! `verify`. The private key never goes in the repo. `new` is for a new app
//! only (installed copies trust the key they were built with); it writes the
//! key only where no file is, and on macOS and Linux readable by the user
//! alone (0600, in a folder it makes 0700); on Windows the file takes the
//! access its folder passes on, as `~\.u4ai-signing` in the user's
//! profile does.

use std::io::Write;

use anyhow::{anyhow, bail, Context};
use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};

/// The public key the updaters trust.
fn trusted_key() -> anyhow::Result<VerifyingKey> {
    let key: [u8; 32] = STANDARD
        .decode(u4ai_core::UPDATE_PUBLIC_KEY)?
        .try_into()
        .map_err(|_| anyhow!("the public key isn't 32 bytes"))?;
    Ok(VerifyingKey::from_bytes(&key)?)
}

fn read_private_key(path: &str) -> anyhow::Result<SigningKey> {
    let text = std::fs::read_to_string(path).with_context(|| format!("reading {path}"))?;
    let seed: [u8; 32] = STANDARD
        .decode(text.trim())
        .with_context(|| format!("{path} isn't base64"))?
        .try_into()
        .map_err(|_| anyhow!("{path} isn't a 32-byte Ed25519 seed"))?;
    Ok(SigningKey::from_bytes(&seed))
}

fn public_text(key: &SigningKey) -> String {
    STANDARD.encode(key.verifying_key().to_bytes())
}

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.iter().map(String::as_str).collect::<Vec<_>>().as_slice() {
        ["new", path] => {
            let mut seed = [0u8; 32];
            getrandom::fill(&mut seed).map_err(|e| anyhow!("no system randomness: {e}"))?;
            let key = SigningKey::from_bytes(&seed);
            if let Some(dir) = std::path::Path::new(path).parent().filter(|d| !d.as_os_str().is_empty()) {
                let mut folder = std::fs::DirBuilder::new();
                folder.recursive(true);
                #[cfg(unix)]
                std::os::unix::fs::DirBuilderExt::mode(&mut folder, 0o700);
                folder.create(dir).with_context(|| format!("making {}", dir.display()))?;
            }
            // create_new: never overwrite a key, since losing it strands every
            // install. Made readable by the user alone from the start, rather
            // than narrowed after it's written, so no other account ever reads
            // the seed.
            let mut options = std::fs::OpenOptions::new();
            options.write(true).create_new(true);
            #[cfg(unix)]
            std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
            let mut file = options.open(path).with_context(|| format!("creating {path} (it must not exist yet)"))?;
            writeln!(file, "{}", STANDARD.encode(seed))?;
            println!("wrote the private key to {path}");
            println!("public key: {}", public_text(&key));
        }
        ["public", path] => println!("{}", public_text(&read_private_key(path)?)),
        ["trusted"] => println!("{}", STANDARD.encode(trusted_key()?.to_bytes())),
        ["sign", file, path] => {
            let key = read_private_key(path)?;
            let data = std::fs::read(file).with_context(|| format!("reading {file}"))?;
            println!("{}", STANDARD.encode(key.sign(&data).to_bytes()));
        }
        ["verify", file, sig] => {
            let sig: [u8; 64] = STANDARD
                .decode(sig.trim())
                .context("the signature isn't base64")?
                .try_into()
                .map_err(|_| anyhow!("the signature isn't 64 bytes"))?;
            let data = std::fs::read(file).with_context(|| format!("reading {file}"))?;
            trusted_key()?
                .verify(&data, &Signature::from_bytes(&sig))
                .map_err(|_| anyhow!("{file}: the signature doesn't match U4Ai's update key"))?;
            println!("{file}: signature OK ({} bytes)", data.len());
        }
        _ => bail!(
            "usage: update_key new <private-key-file> | public <private-key-file> | trusted | \
             sign <file> <private-key-file> | verify <file> <base64 signature>"
        ),
    }
    Ok(())
}
