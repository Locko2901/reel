//! Checks a file against its updater signature and the public key in tauri.conf.json.
//! Usage: verify_signature <tauri.conf.json> <file> [<file>.sig]
use base64::Engine;

fn decoded(b64: &str) -> String {
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(b64.trim())
        .expect("base64");
    String::from_utf8(bytes).expect("text")
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let (conf, file) = (&args[1], &args[2]);
    let sig = args
        .get(3)
        .cloned()
        .unwrap_or_else(|| format!("{file}.sig"));
    let conf: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(conf).expect("the config")).expect("JSON");
    let key = minisign_verify::PublicKey::decode(&decoded(
        conf["plugins"]["updater"]["pubkey"]
            .as_str()
            .expect("a pubkey"),
    ))
    .expect("the public key");
    let signature = minisign_verify::Signature::decode(&decoded(
        &std::fs::read_to_string(&sig).expect("the signature"),
    ))
    .expect("the signature");
    let data = std::fs::read(file).expect("the file");
    match key.verify(&data, &signature, true) {
        Ok(()) => println!("{file}: the signature matches the app's key"),
        Err(e) => {
            eprintln!("{file}: {e}");
            std::process::exit(1);
        }
    }
}
