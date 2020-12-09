use opt::{Opt, Ultraman};
use structopt::StructOpt;

mod cmd;
mod env;
mod log;
mod opt;
mod output;
mod process;
mod procfile;
mod signal;
mod stream_read;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let opt = Opt::from_args();

    if let Some(subcommand) = opt.subcommands {
        match subcommand {
            Ultraman::Start(opts) => cmd::start::run(opts).expect("failed ultraman start"),
            Ultraman::Run(opts) => cmd::run::run(opts),
            Ultraman::Export(opts) => cmd::export::run(opts).expect("failed ultraman export"),
        }
    }

    Ok(())
}
