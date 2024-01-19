use std::{ffi::OsStr, path::{Path, PathBuf}};
use sysinfo::{
	Pid,
	ProcessExt, 
	System, 
	SystemExt
};

use crate::error::Error;

pub fn get_pid_files(dir: &Path) -> Result<Vec<PathBuf>, Error> {
	let ext = OsStr::new("pid");
	let files = dir
		.read_dir()?
		.filter_map(|x| x.ok())
		.filter(|x| x.path().extension() == Some(ext))
		.map(|x| x.path())
		.collect();

	Ok(files)
}

pub fn is_running(pid: usize) -> bool {
	let mut system = System::new();
	system.refresh_processes();
	
	system.process(Pid::from(pid)).is_some()
}

pub fn kill(pid: usize) {
	let mut system = System::new();
	system.refresh_processes();

	let proc = system.process(Pid::from(pid)).expect("Expected a running process.");
	proc.kill();
}

pub fn get_name(pid: usize) -> Option<String> {
	let system = System::new();
	let proc = system.process(Pid::from(pid))?;

	Some(proc.name().to_string())
}
