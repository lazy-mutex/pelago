use std::io::Read;

// Reads Mermaid text from a file, or from standard input, and writes SVG to standard output.
fn main() {
    let mut text = String::new();
    let mut path = None;
    let mut options = pelago::RenderOptions::default();
    for arg in std::env::args().skip(1) {
        match arg.as_str() {
            "--raw" => options.raw = true,
            "--dark" => options.theme = pelago::Theme::dark(),
            "-h" | "--help" => {
                eprintln!(
                    "usage: pelago [--raw] [--dark] [file]\n\n  --raw   keep Pele's CSS variables instead of resolving them to plain colors\n  --dark  resolve colors using the dark palette"
                );
                return;
            }
            _ => path = Some(arg),
        }
    }
    let read = match &path {
        Some(path) => std::fs::read_to_string(path).map(|t| text = t),
        None => std::io::stdin().read_to_string(&mut text).map(|_| ()),
    };
    if let Err(error) = read {
        eprintln!("pelago: {}", error);
        std::process::exit(2);
    }
    match pelago::render(&text, &options) {
        Ok(result) => println!("{}", result.svg),
        Err(error) => {
            eprintln!("pelago: {}", error);
            std::process::exit(1);
        }
    }
}
