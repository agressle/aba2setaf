use std::{ffi::OsString, process::exit};

use aba2setaf::{aba_framework_builder::AbaFrameworkBuilder, translation::translate, EXIT_CODE_INSTANCE, EXIT_CODE_SETUP_SIGNALS, EXIT_CODE_SIGNALS};
use clap::Parser;
use tokio::{select, signal::unix::{signal, SignalKind}};


#[derive(Parser)]
#[command(
    author = "Alexander Greßler <agressle@dbai.tuwien.ac.at>",
    version = env!("CARGO_PKG_VERSION"),
    about = "A tool for converting instances of ABAF to SETAF."
)]
struct Args
{
    #[arg(
        short = 'i',
        long = "instance",
        help = "A file that contains the encoding of the ABA instance.",
        value_name = "FILE",
        required = true)
    ]    
    instance: OsString,

    #[arg(
        short = 'd',
        long = "destination",
        help = "The path to where the output file should be written to.",
        value_name = "FILE",
        required = true)
    ]    
    destination: OsString,

    #[arg(
        short = 'o',
        long = "overwrite",
        help = "When provided, the destination file will be overwritten if it exsits.",
        required = false,
        default_value_t = false)
    ]
    overwrite: bool,

    #[arg(
        short = 'a',
        long = "asp",
        help = "When provided, the asp destination encoding will be used.",
        required = false,
        default_value_t = false)
    ]
    asp: bool
}

#[tokio::main]
async fn main() {
    let exit_code = select! {
        exit_code = signals() => {exit_code}
        exit_code = work() => {exit_code}
    };    
    exit(exit_code);
}

async fn signals() -> i32 {
    let Ok(mut sig_int) = signal(SignalKind::interrupt()) else {
        return EXIT_CODE_SETUP_SIGNALS;
    };
    let Ok(mut sig_term) = signal(SignalKind::terminate()) else {
        return EXIT_CODE_SETUP_SIGNALS;
    };
    let Ok(mut sig_quit) = signal(SignalKind::quit()) else {
        return EXIT_CODE_SETUP_SIGNALS;
    };        

    select! {
        _ = sig_int.recv() => {}
        _ = sig_term.recv() => {}
        _ = sig_quit.recv() => {}
    }

    EXIT_CODE_SIGNALS
}

async fn work() -> i32 {
    let args = Args::parse();
    let aba_framework_builder = match AbaFrameworkBuilder::parse(&args.instance).await {
        Ok(aba_framework_builder) => aba_framework_builder,
        Err(error) => {
            eprintln!("Failed to parse instance file: {}", error);
            return EXIT_CODE_INSTANCE;
        }
    };

    return translate(aba_framework_builder, &args.destination, args.overwrite, args.asp).await;
}