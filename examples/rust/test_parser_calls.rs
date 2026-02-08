use crate::parsing::rust::parser::RustParser;
use crate::parsing::Parser;

fn main() {
    let code = r#"
pub fn init_config_file() {
    crate::init::init_global_dirs();
}

pub fn init_global_dirs() {
    println!("Initializing");
}
"#;

    let mut parser = RustParser::new().unwrap();
    let calls = parser.find_calls(code);
    
    println!("Found {} calls:", calls.len());
    for (from, to, _) in calls {
        println!("  {} -> {}", from, to);
    }
}
