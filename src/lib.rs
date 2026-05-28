use std::process::exit;

pub mod aba_framework;
pub mod aba_framework_builder;
mod aba_rule;
mod trie;
mod solutions;
pub mod translation;
mod tree;

//Constants
pub const EXIT_CODE_OK: i32 = 0;
pub const EXIT_CODE_OTHER_ERROR: i32 = 1;
pub const EXIT_CODE_SETUP_SIGNALS: i32 = 2;
pub const EXIT_CODE_SIGNALS: i32 = 3;
pub const EXIT_CODE_IO_ERROR: i32 = 4;
pub const EXIT_CODE_INSTANCE: i32 = 5;
pub const EXIT_CODE_FILE_EXISTS: i32 = 6;

pub fn on_error(message: &str, return_code: i32) -> ! {
    eprintln!("This is a logic error, please report to the developer: {}", message);
    exit(return_code)
}