use crate::output;
use crate::process;
use crate::procfile::read_procfile;
use crate::signal;

use std::path::PathBuf;
use std::sync::{Arc, Barrier, Mutex};
use structopt::{clap, StructOpt};

#[derive(StructOpt, Debug)]
#[structopt(setting(clap::AppSettings::ColoredHelp))]
pub struct StartOpts {
    /// Formation
    #[structopt(
        name = "APP=NUMBER",
        short = "m",
        long = "formation",
        default_value = "all=1"
    )]
    pub formation: String,

    /// .env file
    #[structopt(
        name = "ENV",
        short = "e",
        long = "env",
        parse(from_os_str),
        default_value = ".env"
    )]
    pub envpath: PathBuf,

    /// Profile path
    #[structopt(
        name = "PROCFILE",
        short = "f",
        long = "file",
        parse(from_os_str),
        default_value = "Procfile"
    )]
    pub procfile_path: PathBuf,

    /// Timeout
    #[structopt(
        name = "TIMEOUT (sec)",
        short = "t",
        long = "timeout",
        default_value = "5",
    )]
    pub timeout: String,

    /// Port
    #[structopt(
        name = "PORT",
        short = "p",
        long = "port",
    )]
    pub port: Option<String>,
}

pub fn run(opts: StartOpts) -> Result<(), Box<dyn std::error::Error>> {
    let mut proc_handles = vec![];
    let procs: Arc<Mutex<Vec<Arc<Mutex<process::Process>>>>> = Arc::new(Mutex::new(vec![]));

    let procfile = read_procfile(opts.procfile_path).expect("failed read Procfile");
    // Read the formation from the command line option and always call it before process_len for the convenience of setting concurrency
    procfile.set_concurrency(&opts.formation);

    let process_len = procfile.process_len();
    let padding = procfile.padding();

    let barrier = Arc::new(Barrier::new(process_len + 1));
    let mut index = 0;

    for (name, pe) in procfile.data.iter() {
        let con = pe.concurrency.get();
        let output = Arc::new(output::Output::new(index, padding));
        index += 1;

        for n in 0..con {
            let barrier = barrier.clone();
            let procs = procs.clone();
            let output = output.clone();
            let name = name.clone();
            let pe_command = pe.command.clone();
            let envpath = opts.envpath.clone();
            let port = opts.port.clone();

            let each_fn = process::each_handle_exec_and_output(procs, padding, barrier, output);
            let each_handle_exec_and_output = each_fn(name, n, pe_command, envpath, port, index);
            proc_handles.push(each_handle_exec_and_output);
        }
    }

    barrier.wait();

    // use handle_signal
    let procs2 = Arc::clone(&procs);
    proc_handles.push(process::check_for_child_termination_thread(procs, padding));

    let procs = Arc::clone(&procs2);
    proc_handles.push(signal::handle_signal_thread(procs, padding, opts.timeout.parse::<u64>().unwrap()));

    for handle in proc_handles {
        handle.join().expect("failed join");
    }

    Ok(())
}
