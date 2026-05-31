// sqr-recover CLI — extract contents from corrupted .sqr files
use std::path::PathBuf;
use squarez::io::v2::recover_v2;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 {
        eprintln!("Usage: sqr-recover <corrupted.sqr> [output_dir]");
        eprintln!("");
        eprintln!("Extract recoverable files from a corrupted .sqr archive.");
        eprintln!("If output_dir is omitted, creates 'recovered/' next to the file.");
        std::process::exit(1);
    }

    let sqr_path = PathBuf::from(&args[1]);
    if !sqr_path.exists() {
        eprintln!("Error: file not found: {}", sqr_path.display());
        std::process::exit(1);
    }

    let out_dir = if args.len() >= 3 {
        PathBuf::from(&args[2])
    } else {
        let parent = sqr_path.parent().unwrap_or(std::path::Path::new("."));
        parent.join("recovered")
    };

    eprintln!("Recovering from: {}", sqr_path.display());
    eprintln!("Output to:       {}", out_dir.display());

    match recover_v2(&sqr_path, &out_dir) {
        Ok(()) => {
            eprintln!("Recovery complete. Files written to {}", out_dir.display());
        }
        Err(e) => {
            eprintln!("Recovery failed: {}", e);
            std::process::exit(1);
        }
    }
}
