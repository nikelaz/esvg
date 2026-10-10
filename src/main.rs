use std::{env, fs, process};

use esvg_rs::{optimize, PluginConfig};

fn main() {
    let args: Vec<String> = env::args().collect();

    if args.len() < 3 {
        eprintln!("Usage: {} <input_file> <output_file>", args[0]);
        process::exit(1);
    }

    let file_path = &args[1];
    let out_path = &args[2];

    let svg_string = fs::read_to_string(file_path).unwrap_or_else(|e| {
        eprintln!("Failed to read {}: {}", file_path, e);
        process::exit(1);
    });

    let svg_output = optimize(&svg_string, &PluginConfig::default()).unwrap_or_else(|e| {
        eprintln!("Failed to optimize {}: {}", file_path, e);
        process::exit(1);
    });

    println!("Input Size: {}", svg_string.len());
    println!("Output Size: {}", svg_output.len());

    fs::write(out_path, svg_output).unwrap_or_else(|e| {
        eprintln!("Failed to write {}: {}", out_path, e);
        process::exit(1);
    });
}
