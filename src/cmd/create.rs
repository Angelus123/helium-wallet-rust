use crate::{
    cmd::*,
    format::{self, Format},
    keypair::{KeyTag, KeyType, Keypair, Network, KEYTYPE_ED25519_STR, NETTYPE_MAIN_STR},
    mnemonic::{mnemonic_to_entropy, SeedType},
    pwhash::PwHash,
    result::Result,
    wallet::Wallet,
};
use std::{
    fs, io,
    path::{Path, PathBuf},
};

#[derive(Debug, StructOpt)]
/// Create a new wallet
pub enum Cmd {
    Basic(Basic),
    Sharded(Sharded),
}

#[derive(Debug, StructOpt)]
/// Create a new basic wallet
pub struct Basic {
    #[structopt(short, long, default_value = "wallet.key")]
    /// Output file to store the key in
    output: PathBuf,

    #[structopt(long)]
    /// Overwrite an existing file
    force: bool,

    #[structopt(long, possible_values = &["bip39", "mobile"], case_insensitive = true)]
    /// Use a BIP39 or mobile app seed phrase to generate the wallet keys
    seed: Option<SeedType>,

    #[structopt(long, default_value = NETTYPE_MAIN_STR)]
    /// The network to generate the wallet (testnet/mainnet)
    network: Network,

    #[structopt(long, default_value = KEYTYPE_ED25519_STR)]
    /// The type of key to generate (ecc_compact/ed25519)
    key_type: KeyType,
}

#[derive(Debug, StructOpt)]
/// Create a new sharded wallet
pub struct Sharded {
    #[structopt(short, long, default_value = "wallet.key")]
    /// Output file to store the key in
    output: PathBuf,

    #[structopt(long)]
    /// Overwrite an existing file
    force: bool,

    #[structopt(short = "n", long = "shards", default_value = "5")]
    /// Number of shards to break the key into
    key_share_count: u8,

    #[structopt(short = "k", long = "required-shards", default_value = "3")]
    /// Number of shards required to recover the key
    recovery_threshold: u8,

    #[structopt(long, possible_values = &["bip39", "mobile"], case_insensitive = true)]
    /// Use a BIP39 or mobile app seed phrase to generate the wallet keys
    seed: Option<SeedType>,

    #[structopt(long, default_value = NETTYPE_MAIN_STR)]
    /// The network to generate the wallet (testnet/mainnet)
    network: Network,

    #[structopt(long, default_value = KEYTYPE_ED25519_STR)]
    /// The type of key to generate (ecc_compact/ed25519)
    key_type: KeyType,
}

impl Cmd {
    pub async fn run(&self, opts: Opts) -> Result {
        match self {
            Cmd::Basic(cmd) => cmd.run(opts).await,
            Cmd::Sharded(cmd) => cmd.run(opts).await,
        }
    }
}

impl Basic {
    pub async fn run(&self, opts: Opts) -> Result {
        let seed_words = match &self.seed {
            Some(seed_type) => Some(get_seed_words(seed_type)?),
            None => None,
        };
        let password = get_password(true)?;
        let tag = KeyTag {
            network: self.network,
            key_type: self.key_type,
        };
        let keypair = gen_keypair(tag, seed_words, self.seed.as_ref())?;
        let format = format::Basic {
            pwhash: PwHash::argon2id13_default(),
        };
        let wallet = Wallet::encrypt(&keypair, password.as_bytes(), Format::Basic(format))?;
        let mut writer = open_output_file(&self.output, !self.force)?;
        wallet.write(&mut writer)?;
        verify::print_result(&wallet, true, opts.format)
    }
}

impl Sharded {
    pub async fn run(&self, opts: Opts) -> Result {
        let seed_words = match &self.seed {
            Some(seed_type) => Some(get_seed_words(seed_type)?),
            None => None,
        };
        let password = get_password(true)?;
        let tag = KeyTag {
            network: self.network,
            key_type: self.key_type,
        };

        let keypair = gen_keypair(tag, seed_words, self.seed.as_ref())?;
        let format = format::Sharded {
            key_share_count: self.key_share_count,
            recovery_threshold: self.recovery_threshold,
            pwhash: PwHash::argon2id13_default(),
            key_shares: vec![],
        };
        let wallet = Wallet::encrypt(&keypair, password.as_bytes(), Format::Sharded(format))?;

        let extension = get_file_extension(&self.output);
        for (i, shard) in wallet.shards()?.iter().enumerate() {
            let mut filename = self.output.clone();
            let share_extension = format!("{}.{}", extension, (i + 1).to_string());
            filename.set_extension(share_extension);
            let mut writer = open_output_file(&filename, !self.force)?;
            shard.write(&mut writer)?;
        }
        verify::print_result(&wallet, true, opts.format)
    }
}

fn gen_keypair(
    tag: KeyTag,
    seed_words: Option<Vec<String>>,
    seed_type: Option<&SeedType>,
) -> Result<Keypair> {
    // Callers of this function should either have Some of both or None of both.
    // Anything else is an error.
    match (seed_words, seed_type) {
        (Some(words), Some(seed_type)) => {
            let entropy = mnemonic_to_entropy(words, seed_type)?;
            Keypair::generate_from_entropy(tag, &entropy)
        }
        (None, None) => Ok(Keypair::generate(tag)),
        _ => bail!("Invalid parameters in gen_keypair(). Report this to the development team."),
    }
}

fn open_output_file(filename: &Path, create: bool) -> io::Result<fs::File> {
    fs::OpenOptions::new()
        .write(true)
        .create(true)
        .create_new(create)
        .open(filename)
}