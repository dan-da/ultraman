use crate::env::read_env;
use crate::log::{self, LogOpt};
use crate::signal;
use nix::sys::signal::Signal;
use nix::sys::wait::WaitStatus;
use nix::{self, unistd::Pid};
use std::env::{self as os_env};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};

#[cfg(not(test))]
use std::process::exit;

pub struct ProcessOpts {
    pub padding: usize,
    pub is_timestamp: bool,
}

pub struct Process {
    pub index: usize,
    pub name: String,
    pub child: Child,
    pub opts: Option<ProcessOpts>,
}

impl Process {
    pub fn new(
        process_name: String,
        cmd: String,
        env_path: PathBuf,
        port: Option<String>,
        concurrency_index: usize,
        index: usize,
        opts: Option<ProcessOpts>,
    ) -> Self {
        let mut read_env = read_env(env_path.clone()).expect("failed read .env");
        read_env.insert(
            String::from("PORT"),
            port_for(env_path, port, index, concurrency_index),
        );
        read_env.insert(
            String::from("PS"),
            ps_for(process_name.clone(), concurrency_index + 1),
        );
        let shell = os_env::var("SHELL").expect("$SHELL is not set");

        Process {
            index,
            name: ps_for(process_name, concurrency_index + 1),
            child: Command::new(shell)
                .arg("-c")
                .arg(cmd)
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .envs(read_env)
                .spawn()
                .expect("failed execute command"),
            opts,
        }
    }
}

// https://stackoverflow.com/questions/34439977/lifetime-of-variables-passed-to-a-new-thread
pub fn build_exec_and_output_thread<F>(yielder: F) -> JoinHandle<()>
where
    F: FnOnce() + Sync + Send + 'static,
{
    thread::Builder::new()
        .name(String::from("handle exec and output"))
        .spawn(move || {
            yielder();
        })
        .expect("failed exec and output")
}

pub fn check_for_child_termination_thread(
    procs: Arc<Mutex<Vec<Arc<Mutex<Process>>>>>,
    padding: usize,
    is_timestamp: bool,
) -> JoinHandle<()> {
    let result = thread::Builder::new()
        .name(String::from(format!("check child terminated")))
        .spawn(move || {
            loop {
                // Waiting for the end of any one child process
                let procs2 = Arc::clone(&procs);
                let procs3 = Arc::clone(&procs);
                if let Some((_, code)) = check_for_child_termination(procs2, padding, is_timestamp)
                {
                    signal::kill_children(procs3, padding, Signal::SIGTERM, code, is_timestamp)
                }
            }
        })
        .expect("failed check child terminated");

    result
}

pub fn check_for_child_termination(
    procs: Arc<Mutex<Vec<Arc<Mutex<Process>>>>>,
    padding: usize,
    is_timestamp: bool,
) -> Option<(Pid, i32)> {
    // Waiting for the end of any one child process
    match nix::sys::wait::waitpid(
        Pid::from_raw(-1),
        Some(nix::sys::wait::WaitPidFlag::WNOHANG),
    ) {
        Ok(exit_status) => match exit_status {
            WaitStatus::Exited(pid, code) => {
                procs.lock().unwrap().retain(|p| {
                    let child_id = p.lock().unwrap().child.id() as i32;
                    if Pid::from_raw(child_id) == pid {
                        let proc = p.lock().unwrap();
                        let proc_name = &proc.name;
                        let proc_index = proc.index;
                        log::output(
                            &proc_name,
                            &format!("exited with code {}", code),
                            padding,
                            Some(proc_index),
                            &LogOpt {
                                is_color: true,
                                is_timestamp,
                            },
                        );
                    }
                    Pid::from_raw(child_id) != pid
                });
                return Some((pid, code));
            }
            WaitStatus::Signaled(pid, signal, _) => {
                procs.lock().unwrap().retain(|p| {
                    let child_id = p.lock().unwrap().child.id() as i32;
                    if Pid::from_raw(child_id) == pid {
                        let proc = p.lock().unwrap();
                        let proc_name = &proc.name;
                        let proc_index = proc.index;
                        log::output(
                            &proc_name,
                            &format!("terminated by {}", signal.as_str()),
                            padding,
                            Some(proc_index),
                            &LogOpt {
                                is_color: true,
                                is_timestamp,
                            },
                        );
                    }
                    Pid::from_raw(child_id) != pid
                });
                return None;
            }
            _ => return None,
        },
        Err(e) => {
            if let nix::Error::Sys(nix::errno::Errno::ECHILD) = e {
                // close loop (thread finished)
                #[cfg(not(test))]
                exit(0);
                #[cfg(test)]
                panic!("exit 0");
            }
            return None;
        }
    };
}

fn ps_for(process_name: String, concurrency: usize) -> String {
    format!("{}.{}", process_name, concurrency)
}

pub fn port_for(
    env_path: PathBuf,
    port: Option<String>,
    index: usize,
    concurrency: usize,
) -> String {
    let result =
        base_port(env_path, port).parse::<usize>().unwrap() + index * 100 + concurrency - 1;
    result.to_string()
}

fn base_port(env_path: PathBuf, port: Option<String>) -> String {
    let env = read_env(env_path).unwrap();
    let default_port = String::from("5000");

    if let Some(p) = port {
        p
    } else if let Some(p) = env.get("PORT") {
        p.clone()
    } else if let Ok(p) = os_env::var("PORT") {
        p
    } else {
        default_port
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[should_panic(expected = "exit 0: Any")]
    fn test_check_for_child_termination_thread() {
        let procs = Arc::new(Mutex::new(vec![
            Arc::new(Mutex::new(Process {
                index: 0,
                name: String::from("check_for_child_termination_thread-1"),
                child: Command::new("./test/fixtures/exit_0.sh")
                    .spawn()
                    .expect("failed execute check_for_child_termination_thread-1"),
                opts: None,
            })),
            Arc::new(Mutex::new(Process {
                index: 1,
                name: String::from("check_for_child_termination_thread-2"),
                child: Command::new("./test/fixtures/exit_1.sh")
                    .spawn()
                    .expect("failed execute check_for_child_termination_thread-2"),
                opts: None,
            })),
        ]));
        let procs2 = Arc::clone(&procs);
        let padding = 10;

        check_for_child_termination_thread(procs2, padding, true)
            .join()
            .expect("exit 0");
    }
}
