// Build script to generate icon fonts at compile time

fn main() {
    // Rebuild if the icon font definition changes
    println!("cargo::rerun-if-changed=fonts/icons.toml");
    
    // Generate the icon font and module
    iced_fontello::build("fonts/icons.toml").expect("Failed to build icon font");
}